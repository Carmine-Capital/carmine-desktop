use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_cache::CacheManager;
use cloudmount_graph::GraphClient;
use cloudmount_vfs::core_ops::CoreOps;
use cloudmount_vfs::inode::InodeTable;

const DRIVE_ID: &str = "test-drive";
const ROOT_ITEM_ID: &str = "root-id";
const FILE_ITEM_ID: &str = "file-1";

fn make_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}

fn unique_cache_dir(prefix: &str) -> std::path::PathBuf {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cloudmount-oft-{prefix}-{id}"))
}

fn make_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = unique_cache_dir(prefix);
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    (cache, base)
}

fn setup_core_ops(graph: Arc<GraphClient>, cache: Arc<CacheManager>) -> Arc<CoreOps> {
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        cloudmount_core::types::DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(cloudmount_core::types::FolderFacet { child_count: 0 }),
            file: None,
            download_url: None,
        },
    );

    let file_ino = inodes.allocate(FILE_ITEM_ID);
    cache.memory.insert(
        file_ino,
        cloudmount_core::types::DriveItem {
            id: FILE_ITEM_ID.to_string(),
            name: "hello.txt".to_string(),
            size: 13,
            last_modified: None,
            created: None,
            etag: Some("etag-1".to_string()),
            parent_reference: Some(cloudmount_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(cloudmount_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            download_url: None,
        },
    );

    Arc::new(CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt))
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
}

async fn mock_file_download(server: &MockServer, content: &[u8]) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(content.to_vec(), "application/octet-stream"),
        )
        .mount(server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_returns_unique_handles() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("unique-handles");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh1 = ops2.open_file(2).unwrap();
        let fh2 = ops2.open_file(2).unwrap();
        assert_ne!(fh1, fh2, "each open should return a unique handle");
        let _ = ops2.release_file(fh1);
        let _ = ops2.release_file(fh2);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn read_slices_from_buffer() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("read-slice");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();

        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");

        let data = ops2.read_handle(fh, 7, 5).unwrap();
        assert_eq!(data, b"world");

        let data = ops2.read_handle(fh, 100, 10).unwrap();
        assert!(data.is_empty());

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn write_mutates_buffer_in_place() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("write-inplace");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();

        let written = ops2.write_handle(fh, 7, b"Rust!").unwrap();
        assert_eq!(written, 5);

        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, Rust!!");

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn write_extends_buffer() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hi").await;

    let (cache, base) = make_cache("write-extend");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();

        let written = ops2.write_handle(fh, 5, b"there").unwrap();
        assert_eq!(written, 5);

        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(&data[0..2], b"Hi");
        assert_eq!(&data[2..5], &[0, 0, 0]);
        assert_eq!(&data[5..10], b"there");

        let item = ops2.lookup_item(2).unwrap();
        assert_eq!(item.size, 10);

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn flush_pushes_to_writeback() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"original").await;

    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "hello.txt",
            "size": 8,
            "etag": "etag-1",
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}:/hello.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "hello.txt",
            "size": 7,
            "etag": "etag-2",
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("flush-wb");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    // Check writeback is empty before flush
    let wb_content = cache.writeback.read(DRIVE_ID, FILE_ITEM_ID).await;
    assert!(wb_content.is_none(), "writeback should be empty initially");

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        ops2.write_handle(fh, 0, b"changed").unwrap();
        ops2.flush_handle(fh).unwrap();
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_dirty_handle_pushes_to_writeback() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"data").await;

    let (cache, base) = make_cache("release-dirty");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        ops2.write_handle(fh, 0, b"modified").unwrap();
        ops2.release_file(fh).unwrap();
    })
    .await
    .unwrap();

    let wb = cache.writeback.read(DRIVE_ID, FILE_ITEM_ID).await;
    assert_eq!(wb.unwrap(), b"modified");

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_clean_handle_no_writeback() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"data").await;

    let (cache, base) = make_cache("release-clean");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 10).unwrap();
        assert_eq!(data, b"data");
        ops2.release_file(fh).unwrap();
    })
    .await
    .unwrap();

    let wb = cache.writeback.read(DRIVE_ID, FILE_ITEM_ID).await;
    assert!(
        wb.is_none(),
        "clean release should not create writeback entry"
    );

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn truncate_on_open_file() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("truncate-open");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();

        ops2.truncate(2, 5).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello");

        ops2.truncate(2, 8).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello\0\0\0");

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_file_returns_open_handle() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}/children"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [] })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("create-handle");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let (fh, ino, item) = ops2.create_file(1, "newfile.txt").unwrap();
        assert!(fh > 0, "file handle should be non-zero");
        assert!(ino > 0, "inode should be non-zero");
        assert_eq!(item.name, "newfile.txt");

        let written = ops2.write_handle(fh, 0, b"new content").unwrap();
        assert_eq!(written, 11);

        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"new content");

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}
