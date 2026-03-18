use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use carminedesktop_cache::CacheManager;
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::core_ops::{CoreOps, StreamingBuffer};
use carminedesktop_vfs::inode::InodeTable;

const DRIVE_ID: &str = "test-drive";
const ROOT_ITEM_ID: &str = "root-id";
const FILE_ITEM_ID: &str = "file-1";
const FILE2_ITEM_ID: &str = "file-2";

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
    std::env::temp_dir().join(format!("carminedesktop-oft-{prefix}-{id}"))
}

fn make_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = unique_cache_dir(prefix);
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300), "test-drive".to_string()).unwrap());
    (cache, base)
}

fn setup_core_ops(graph: Arc<GraphClient>, cache: Arc<CacheManager>) -> Arc<CoreOps> {
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        carminedesktop_core::types::DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(carminedesktop_core::types::FolderFacet { child_count: 0 }),
            file: None,
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let file_ino = inodes.allocate(FILE_ITEM_ID);
    cache.memory.insert(
        file_ino,
        carminedesktop_core::types::DriveItem {
            id: FILE_ITEM_ID.to_string(),
            name: "hello.txt".to_string(),
            size: 13,
            last_modified: None,
            created: None,
            etag: Some("etag-1".to_string()),
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
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
            "eTag": "etag-1",
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
        ops2.flush_handle(fh, false).unwrap();
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

// --- copy_file_range tests ---

fn setup_core_ops_two_files(
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
) -> (Arc<CoreOps>, u64, u64) {
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        carminedesktop_core::types::DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(carminedesktop_core::types::FolderFacet { child_count: 0 }),
            file: None,
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let ino1 = inodes.allocate(FILE_ITEM_ID);
    cache.memory.insert(
        ino1,
        carminedesktop_core::types::DriveItem {
            id: FILE_ITEM_ID.to_string(),
            name: "source.txt".to_string(),
            size: 11,
            last_modified: None,
            created: None,
            etag: Some("etag-1".to_string()),
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let ino2 = inodes.allocate(FILE2_ITEM_ID);
    cache.memory.insert(
        ino2,
        carminedesktop_core::types::DriveItem {
            id: FILE2_ITEM_ID.to_string(),
            name: "dest.txt".to_string(),
            size: 5,
            last_modified: None,
            created: None,
            etag: Some("etag-2".to_string()),
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let ops = Arc::new(CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt));
    (ops, ino1, ino2)
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_file_range_eligible_when_remote_full_file() {
    let server = MockServer::start().await;

    // Source file download
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"hello world".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    // Dest file download
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE2_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(b"12345".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    // copy endpoint
    let monitor_url = format!("{}/monitor/copy1", server.uri());
    Mock::given(method("POST"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/copy"
        )))
        .respond_with(ResponseTemplate::new(202).insert_header("Location", monitor_url.as_str()))
        .mount(&server)
        .await;

    // monitor poll
    Mock::given(method("GET"))
        .and(path("/monitor/copy1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "resourceId": "new-copied-id",
        })))
        .mount(&server)
        .await;

    // get new item
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/new-copied-id")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "new-copied-id",
            "name": "dest.txt",
            "size": 11,
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("copy-eligible");
    let graph = make_graph(&server.uri());
    let (ops, ino1, ino2) = setup_core_ops_two_files(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh_in = ops2.open_file(ino1).unwrap();
        let fh_out = ops2.open_file(ino2).unwrap();

        // full-file, remote source → server-side copy
        let copied = ops2
            .copy_file_range(ino1, fh_in, 0, ino2, fh_out, 0, 11)
            .unwrap();
        assert_eq!(copied, 11);

        let _ = ops2.release_file(fh_in);
        let _ = ops2.release_file(fh_out);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_file_range_ineligible_local_source() {
    let server = MockServer::start().await;

    let (cache, base) = make_cache("copy-local-src");
    let graph = make_graph(&server.uri());

    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        carminedesktop_core::types::DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(carminedesktop_core::types::FolderFacet { child_count: 0 }),
            file: None,
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    // Local source file
    let local_id = "local:12345";
    let ino_src = inodes.allocate(local_id);
    cache.memory.insert(
        ino_src,
        carminedesktop_core::types::DriveItem {
            id: local_id.to_string(),
            name: "local.txt".to_string(),
            size: 5,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let ino_dst = inodes.allocate(FILE2_ITEM_ID);
    cache.memory.insert(
        ino_dst,
        carminedesktop_core::types::DriveItem {
            id: FILE2_ITEM_ID.to_string(),
            name: "dest.txt".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    // Pre-populate writeback for local source so open_file doesn't try to download
    cache
        .writeback
        .write(DRIVE_ID, local_id, b"local")
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE2_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(Vec::new(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let ops = Arc::new(CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt));

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh_in = ops2.open_file(ino_src).unwrap();
        let fh_out = ops2.open_file(ino_dst).unwrap();

        // local: source → should fallback to buffer copy
        let copied = ops2
            .copy_file_range(ino_src, fh_in, 0, ino_dst, fh_out, 0, 5)
            .unwrap();
        assert_eq!(copied, 5);

        // Verify buffer-level copy worked
        let data = ops2.read_handle(fh_out, 0, 100).unwrap();
        assert_eq!(data, b"local");

        let _ = ops2.release_file(fh_in);
        let _ = ops2.release_file(fh_out);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_file_range_ineligible_partial_offset() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"hello world".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE2_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(b"12345".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("copy-partial");
    let graph = make_graph(&server.uri());
    let (ops, ino1, ino2) = setup_core_ops_two_files(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh_in = ops2.open_file(ino1).unwrap();
        let fh_out = ops2.open_file(ino2).unwrap();

        // offset_in > 0 → fallback
        let copied = ops2
            .copy_file_range(ino1, fh_in, 6, ino2, fh_out, 0, 5)
            .unwrap();
        assert_eq!(copied, 5);

        let data = ops2.read_handle(fh_out, 0, 5).unwrap();
        assert_eq!(data, b"world");

        let _ = ops2.release_file(fh_in);
        let _ = ops2.release_file(fh_out);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_file_range_ineligible_len_too_small() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"hello world".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE2_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(b"12345".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("copy-len-small");
    let graph = make_graph(&server.uri());
    let (ops, ino1, ino2) = setup_core_ops_two_files(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh_in = ops2.open_file(ino1).unwrap();
        let fh_out = ops2.open_file(ino2).unwrap();

        // len < source size → fallback
        let copied = ops2
            .copy_file_range(ino1, fh_in, 0, ino2, fh_out, 0, 5)
            .unwrap();
        assert_eq!(copied, 5);

        let data = ops2.read_handle(fh_out, 0, 5).unwrap();
        assert_eq!(data, b"hello");

        let _ = ops2.release_file(fh_in);
        let _ = ops2.release_file(fh_out);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// --- Streaming download tests ---

const LARGE_FILE_ID: &str = "large-file-1";
// 5 MB — above the 4 MB SMALL_FILE_LIMIT threshold
const LARGE_FILE_SIZE: usize = 5 * 1024 * 1024;

fn setup_core_ops_large_file(
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
) -> (Arc<CoreOps>, u64) {
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        carminedesktop_core::types::DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(carminedesktop_core::types::FolderFacet { child_count: 0 }),
            file: None,
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let file_ino = inodes.allocate(LARGE_FILE_ID);
    cache.memory.insert(
        file_ino,
        carminedesktop_core::types::DriveItem {
            id: LARGE_FILE_ID.to_string(),
            name: "bigfile.bin".to_string(),
            size: LARGE_FILE_SIZE as i64,
            last_modified: None,
            created: None,
            etag: Some("etag-large".to_string()),
            parent_reference: Some(carminedesktop_core::types::ParentReference {
                drive_id: Some(DRIVE_ID.to_string()),
                id: Some(ROOT_ITEM_ID.to_string()),
                path: None,
            }),
            folder: None,
            file: Some(carminedesktop_core::types::FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let ops = Arc::new(CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt));
    (ops, file_ino)
}

fn large_file_content() -> Vec<u8> {
    (0..LARGE_FILE_SIZE).map(|i| (i % 251) as u8).collect()
}

// Task 2.8: StreamingBuffer unit tests

#[tokio::test]
async fn streaming_buffer_append_updates_progress() {
    let buf = StreamingBuffer::new(100).unwrap();
    assert_eq!(buf.downloaded_bytes(), 0);

    buf.append_chunk(&[1, 2, 3]).await;
    assert_eq!(buf.downloaded_bytes(), 3);

    buf.append_chunk(&[4, 5]).await;
    assert_eq!(buf.downloaded_bytes(), 5);

    let data = buf.read_range(0, 5).await;
    assert_eq!(data, vec![1, 2, 3, 4, 5]);
}

#[tokio::test(flavor = "multi_thread")]
async fn streaming_buffer_wait_for_range_blocks_until_data() {
    let buf = Arc::new(StreamingBuffer::new(100).unwrap());
    let buf2 = buf.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        buf2.append_chunk(&[0u8; 50]).await;
        buf2.mark_done();
    });

    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        buf.wait_for_range(0, 50, &rt).unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn streaming_buffer_wait_for_range_returns_error_on_failed() {
    let buf = Arc::new(StreamingBuffer::new(100).unwrap());
    let buf2 = buf.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        buf2.mark_failed("network error".to_string());
    });

    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let result = buf.wait_for_range(0, 100, &rt);
        assert!(result.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn streaming_buffer_read_range_correct_slices() {
    let buf = StreamingBuffer::new(10).unwrap();
    buf.append_chunk(&[10, 20, 30, 40, 50]).await;

    let data = buf.read_range(2, 2).await;
    assert_eq!(data, vec![30, 40]);

    let data = buf.read_range(8, 2).await;
    assert!(data.is_empty());
}

// StreamingBuffer size cap tests (Fix 3)

#[tokio::test]
async fn streaming_buffer_rejects_zero_size() {
    let result = StreamingBuffer::new(0);
    assert!(result.is_err(), "StreamingBuffer should reject size 0");
}

#[tokio::test]
async fn streaming_buffer_rejects_oversized() {
    let result = StreamingBuffer::new(256 * 1024 * 1024 + 1);
    assert!(
        result.is_err(),
        "StreamingBuffer should reject sizes > 256MB"
    );
}

#[tokio::test]
async fn streaming_buffer_accepts_max_valid_size() {
    let result = StreamingBuffer::new(256 * 1024 * 1024);
    assert!(
        result.is_ok(),
        "StreamingBuffer should accept exactly 256MB"
    );
}

// Task 11.1: DownloadState transition tests

#[tokio::test]
async fn download_state_complete_reads_work() {
    use carminedesktop_vfs::core_ops::DownloadState;

    let state = DownloadState::Complete(vec![1, 2, 3, 4, 5]);
    assert!(state.is_complete());
    assert_eq!(state.as_complete().unwrap(), &vec![1, 2, 3, 4, 5]);
}

#[tokio::test(flavor = "multi_thread")]
async fn download_state_streaming_transitions_to_complete() {
    use carminedesktop_vfs::core_ops::DownloadState;

    let buf = Arc::new(StreamingBuffer::new(5).unwrap());
    buf.append_chunk(&[1, 2, 3, 4, 5]).await;
    buf.mark_done();

    let task = tokio::spawn(async {});
    let state = DownloadState::Streaming {
        buffer: buf.clone(),
        task: task.abort_handle(),
    };

    assert!(!state.is_complete());

    let buf2 = buf.clone();
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        buf2.wait_for_range(0, 5, &rt).unwrap();
    })
    .await
    .unwrap();
    let data = buf.read_range(0, 5).await;
    assert_eq!(data, vec![1, 2, 3, 4, 5]);

    drop(state);
}

// Task 11.2: Streaming open/read lifecycle

#[tokio::test(flavor = "multi_thread")]
async fn streaming_open_read_lifecycle() {
    let server = MockServer::start().await;
    let content = large_file_content();

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{LARGE_FILE_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(content.clone(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stream-lifecycle");
    let graph = make_graph(&server.uri());
    let (ops, file_ino) = setup_core_ops_large_file(graph, cache);

    let ops2 = ops.clone();
    let content2 = content.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(file_ino).unwrap();

        let data = ops2.read_handle(fh, 0, 4096).unwrap();
        assert_eq!(data.len(), 4096);
        assert_eq!(&data[..], &content2[..4096]);

        let data = ops2.read_handle(fh, 1024, 2048).unwrap();
        assert_eq!(data.len(), 2048);
        assert_eq!(&data[..], &content2[1024..3072]);

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// Task 11.3: Cancellation test

#[tokio::test(flavor = "multi_thread")]
async fn streaming_cancellation_no_disk_cache() {
    let server = MockServer::start().await;
    let content = large_file_content();

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{LARGE_FILE_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(content, "application/octet-stream")
                .set_delay(tokio::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stream-cancel");
    let graph = make_graph(&server.uri());
    let (ops, file_ino) = setup_core_ops_large_file(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(file_ino).unwrap();
        ops2.release_file(fh).unwrap();
    })
    .await
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let cached = cache.disk.get(DRIVE_ID, LARGE_FILE_ID).await;
    assert!(
        cached.is_none(),
        "cancelled download should not populate disk cache"
    );

    cleanup(&base);
}

// Task 11.4: Random-access read

#[tokio::test(flavor = "multi_thread")]
async fn streaming_random_access_uses_range_request() {
    let server = MockServer::start().await;
    let content = large_file_content();

    let range_offset = 4 * 1024 * 1024;
    let range_size = 1024;
    let range_content = content[range_offset..range_offset + range_size].to_vec();

    // Only mount the range-request mock. The streaming download (no Range header)
    // will get a 404 from wiremock and mark the buffer as Failed.
    // read_handle at offset 4MB sees downloaded_bytes()=0, exceeds the 2MB
    // random-access threshold, and issues a range request instead.
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{LARGE_FILE_ID}/content"
        )))
        .and(header_exists("range"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_raw(range_content.clone(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stream-random");
    let graph = make_graph(&server.uri());
    let (ops, file_ino) = setup_core_ops_large_file(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(file_ino).unwrap();

        // Give background task a moment to fail on 404
        std::thread::sleep(std::time::Duration::from_millis(100));

        let data = ops2.read_handle(fh, range_offset, range_size).unwrap();
        assert_eq!(data.len(), range_size);
        assert_eq!(data, range_content);

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// Task 11.5: Download failure propagation

#[tokio::test(flavor = "multi_thread")]
async fn streaming_download_failure_propagates_to_read() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{LARGE_FILE_ID}/content"
        )))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": {
                "code": "internalServerError",
                "message": "Server exploded"
            }
        })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stream-fail");
    let graph = make_graph(&server.uri());
    let (ops, file_ino) = setup_core_ops_large_file(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(file_ino).unwrap();

        let result = ops2.read_handle(fh, 0, 4096);
        assert!(
            result.is_err(),
            "read should return error when download fails"
        );

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// Task 11.6: Write to streaming file

#[tokio::test(flavor = "multi_thread")]
async fn write_to_streaming_file_blocks_until_complete() {
    let server = MockServer::start().await;
    let content = large_file_content();

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{LARGE_FILE_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(content.clone(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stream-write");
    let graph = make_graph(&server.uri());
    let (ops, file_ino) = setup_core_ops_large_file(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(file_ino).unwrap();

        let written = ops2.write_handle(fh, 0, b"PATCHED!").unwrap();
        assert_eq!(written, 8);

        let data = ops2.read_handle(fh, 0, 8).unwrap();
        assert_eq!(data, b"PATCHED!");

        let data = ops2.read_handle(fh, 8, 100).unwrap();
        assert_eq!(&data[..], &content[8..108]);

        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_file_range_fallback_copies_at_offsets() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"ABCDEFGHIJ".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE2_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"1234567890".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("copy-offsets");
    let graph = make_graph(&server.uri());
    let (ops, ino1, ino2) = setup_core_ops_two_files(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh_in = ops2.open_file(ino1).unwrap();
        let fh_out = ops2.open_file(ino2).unwrap();

        // Copy 3 bytes from offset 2 in source to offset 5 in dest
        let copied = ops2
            .copy_file_range(ino1, fh_in, 2, ino2, fh_out, 5, 3)
            .unwrap();
        assert_eq!(copied, 3);

        let data = ops2.read_handle(fh_out, 0, 10).unwrap();
        assert_eq!(&data[0..5], b"12345");
        assert_eq!(&data[5..8], b"CDE");
        assert_eq!(&data[8..10], b"90");

        let _ = ops2.release_file(fh_in);
        let _ = ops2.release_file(fh_out);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// --- Cache freshness validation tests ---

#[tokio::test(flavor = "multi_thread")]
async fn open_file_with_stale_disk_cache_wrong_size_triggers_redownload() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("stale-size");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    // Pre-populate disk cache with wrong-size content (5 bytes vs metadata says 13)
    cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"stale", Some("etag-1"))
        .await
        .unwrap();

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn open_file_with_stale_etag_triggers_redownload() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("stale-etag");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    // Pre-populate disk cache with matching size but wrong eTag
    cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"Hello, world!", Some("etag-old"))
        .await
        .unwrap();

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn open_file_with_dirty_inode_skips_disk_cache() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("dirty-inode");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    // Pre-populate disk cache with valid content and matching eTag
    cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"Hello, world!", Some("etag-1"))
        .await
        .unwrap();

    // Mark the inode as dirty
    cache.dirty_inodes.insert(2);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");
        // Dirty flag should be cleared after download
        assert!(!ops2.is_dirty(2));
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn open_file_with_valid_cache_serves_from_disk() {
    let server = MockServer::start().await;
    // No download mock — if disk cache is valid, no network call should happen

    let (cache, base) = make_cache("valid-cache");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    // Pre-populate disk cache with correct content, correct eTag, correct size
    cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"Hello, world!", Some("etag-1"))
        .await
        .unwrap();

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// --- Conflict naming tests ---

#[test]
fn conflict_name_preserves_extension() {
    use carminedesktop_vfs::core_ops::conflict_name;
    let result = conflict_name("report.docx", 1741000000);
    assert_eq!(result, "report.conflict.1741000000.docx");
}

#[test]
fn conflict_name_no_extension() {
    use carminedesktop_vfs::core_ops::conflict_name;
    let result = conflict_name("Makefile", 1741000000);
    assert_eq!(result, "Makefile.conflict.1741000000");
}

#[test]
fn conflict_name_multiple_dots() {
    use carminedesktop_vfs::core_ops::conflict_name;
    let result = conflict_name("archive.tar.gz", 1741000000);
    assert_eq!(result, "archive.tar.conflict.1741000000.gz");
}

#[test]
fn conflict_name_hidden_file_with_extension() {
    use carminedesktop_vfs::core_ops::conflict_name;
    let result = conflict_name(".config.json", 1741000000);
    assert_eq!(result, ".config.conflict.1741000000.json");
}

// --- OpenFileTable extension tests (stale reads prevention) ---

#[test]
fn test_open_file_table_get_content_size_by_ino_complete() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();
    let fh = table.insert(42, DownloadState::Complete(vec![0u8; 5000]));
    assert_eq!(table.get_content_size_by_ino(42), Some(5000));
    table.remove(fh);
}

#[tokio::test]
async fn test_open_file_table_get_content_size_by_ino_streaming() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable, StreamingBuffer};
    let table = OpenFileTable::new();
    let buffer = Arc::new(StreamingBuffer::new(10000).unwrap());
    let task = tokio::spawn(async {});
    let fh = table.insert(
        42,
        DownloadState::Streaming {
            buffer,
            task: task.abort_handle(),
        },
    );
    assert_eq!(table.get_content_size_by_ino(42), Some(10000));
    table.remove(fh);
}

#[test]
fn test_open_file_table_get_content_size_by_ino_no_match() {
    use carminedesktop_vfs::core_ops::OpenFileTable;
    let table = OpenFileTable::new();
    assert_eq!(table.get_content_size_by_ino(99), None);
}

#[test]
fn test_open_file_table_get_content_size_by_ino_multiple_handles() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();
    let _fh1 = table.insert(42, DownloadState::Complete(vec![0u8; 3000]));
    let _fh2 = table.insert(42, DownloadState::Complete(vec![0u8; 5000]));
    // Returns the first match (either 3000 or 5000 depending on iteration order)
    let size = table.get_content_size_by_ino(42);
    assert!(size == Some(3000) || size == Some(5000));
}

#[test]
fn test_open_file_table_mark_stale_by_ino_sets_flag() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();
    let fh = table.insert(42, DownloadState::Complete(vec![1, 2, 3]));

    // Verify not stale initially
    assert!(!table.get(fh).unwrap().stale);

    table.mark_stale_by_ino(42);

    // Verify stale after marking
    assert!(table.get(fh).unwrap().stale);
    table.remove(fh);
}

#[test]
fn test_open_file_table_mark_stale_by_ino_no_effect_on_other_inodes() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();
    let fh1 = table.insert(42, DownloadState::Complete(vec![1, 2, 3]));
    let fh2 = table.insert(99, DownloadState::Complete(vec![4, 5, 6]));

    table.mark_stale_by_ino(42);

    assert!(table.get(fh1).unwrap().stale, "inode 42 should be stale");
    assert!(
        !table.get(fh2).unwrap().stale,
        "inode 99 should NOT be stale"
    );
    table.remove(fh1);
    table.remove(fh2);
}

#[test]
fn test_open_file_table_mark_stale_by_ino_all_handles_for_same_inode() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();
    let fh1 = table.insert(42, DownloadState::Complete(vec![1, 2, 3]));
    let fh2 = table.insert(42, DownloadState::Complete(vec![4, 5, 6]));

    table.mark_stale_by_ino(42);

    assert!(
        table.get(fh1).unwrap().stale,
        "first handle should be stale"
    );
    assert!(
        table.get(fh2).unwrap().stale,
        "second handle should be stale"
    );
    table.remove(fh1);
    table.remove(fh2);
}

#[test]
fn test_open_file_table_has_open_handles() {
    use carminedesktop_vfs::core_ops::{DownloadState, OpenFileTable};
    let table = OpenFileTable::new();

    assert!(!table.has_open_handles(42));

    let fh = table.insert(42, DownloadState::Complete(vec![1]));
    assert!(table.has_open_handles(42));
    assert!(!table.has_open_handles(99));

    table.remove(fh);
    assert!(!table.has_open_handles(42));
}

// --- Handle-consistent getattr test ---

#[tokio::test(flavor = "multi_thread")]
async fn test_lookup_item_for_getattr_returns_handle_size() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("getattr-handle");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        // Open file — handle holds 13 bytes
        let fh = ops2.open_file(2).unwrap();

        // Simulate delta sync updating cache with a new size
        let mut item = ops2.lookup_item(2).unwrap();
        item.size = 7000; // Server reports 7000 bytes
        ops2.cache().memory.insert(2, item);

        // lookup_item_for_getattr should return handle size (13), not cache size (7000)
        let (attr_item, has_handle) = ops2.lookup_item_for_getattr(2).unwrap();
        assert!(has_handle, "should detect open handle");
        assert_eq!(
            attr_item.size, 13,
            "getattr should return handle content size, not cache size"
        );

        // After releasing, should fall back to cache size
        let _ = ops2.release_file(fh);
        let (attr_item, has_handle) = ops2.lookup_item_for_getattr(2).unwrap();
        assert!(!has_handle, "no open handle after release");
        assert_eq!(
            attr_item.size, 7000,
            "should return cache size when no handle"
        );
    })
    .await
    .unwrap();

    cleanup(&base);
}

// ============================================================================
// SERVER METADATA REFRESH — stale cache and offline fallback
// ============================================================================

/// Helper: mock get_item endpoint returning fresh metadata with a new eTag
async fn mock_get_item_fresh(server: &MockServer, etag: &str, size: i64) {
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "hello.txt",
            "size": size,
            "eTag": etag,
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_file_serves_fresh_content_when_server_etag_differs() {
    let server = MockServer::start().await;

    // Server returns updated metadata (etag-2, size 11)
    mock_get_item_fresh(&server, "etag-2", 11).await;

    // Download endpoint returns the NEW content
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"new content".to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let (cache, base) = make_cache("stale-cache-refresh");
    let graph = make_graph(&server.uri());

    // Pre-populate disk cache with OLD content and OLD etag
    let _ = cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"Hello, world!", Some("etag-1"))
        .await;

    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(
            data, b"new content",
            "should serve fresh content, not stale disk cache"
        );
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn open_file_falls_back_to_disk_cache_when_get_item_fails() {
    let server = MockServer::start().await;

    // No get_item mock — the call will fail (404 from wiremock)

    let (cache, base) = make_cache("offline-fallback");
    let graph = make_graph(&server.uri());

    // Pre-populate disk cache with content matching the cached metadata
    let _ = cache
        .disk
        .put(DRIVE_ID, FILE_ITEM_ID, b"Hello, world!", Some("etag-1"))
        .await;

    // Memory cache has etag-1, size 13 (from setup_core_ops)
    // Disk cache has etag-1 and 13 bytes — should match
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(
            data, b"Hello, world!",
            "should fall back to disk cache when get_item fails"
        );
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}

// ============================================================================
// FLUSH INODE — 423 Locked handling
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn flush_inode_creates_conflict_copy_on_423_locked() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"original").await;

    // get_item for conflict check — eTags match (no prior conflict)
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "hello.txt",
            "size": 8,
            "eTag": "etag-1",
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    // Main upload has If-Match header (from conflict check) → 423 Locked
    Mock::given(method("PUT"))
        .and(header_exists("If-Match"))
        .respond_with(ResponseTemplate::new(423).set_body_json(json!({
            "error": {
                "code": "notAllowed",
                "message": "The resource you are attempting to access is locked"
            }
        })))
        .mount(&server)
        .await;

    // Conflict copy has no If-Match header → 200 success
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "conflict-copy-id",
            "name": "hello.conflict.123.txt",
            "size": 7,
        })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("flush-423");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        ops2.write_handle(fh, 0, b"changed").unwrap();
        // flush_handle returns Ok when conflict copy upload succeeds
        // (data is safe as conflict copy, dirty is cleared)
        ops2.flush_handle(fh, false)
            .expect("flush should succeed after conflict copy upload");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    // Writeback buffer should be cleared (conflict copy was uploaded)
    let wb = cache.writeback.read(DRIVE_ID, FILE_ITEM_ID).await;
    assert!(
        wb.is_none(),
        "writeback should be cleared after conflict copy upload"
    );

    cleanup(&base);
}

#[tokio::test(flavor = "multi_thread")]
async fn flush_inode_preserves_writeback_when_conflict_copy_also_fails() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"original").await;

    // get_item for conflict check — eTags match
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "hello.txt",
            "size": 8,
            "eTag": "etag-1",
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    // Catch-all PUT → 500 (conflict copy also fails)
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    // Main upload has If-Match header → 423 Locked
    Mock::given(method("PUT"))
        .and(header_exists("If-Match"))
        .respond_with(ResponseTemplate::new(423).set_body_json(json!({
            "error": {
                "code": "notAllowed",
                "message": "locked"
            }
        })))
        .mount(&server)
        .await;

    let (cache, base) = make_cache("flush-423-copy-fail");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache.clone());

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        ops2.write_handle(fh, 0, b"changed").unwrap();
        let result = ops2.flush_handle(fh, false);
        assert!(result.is_err(), "flush should fail");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    // Writeback buffer should still have content (crash recovery safety net)
    let wb = cache.writeback.read(DRIVE_ID, FILE_ITEM_ID).await;
    assert!(
        wb.is_some(),
        "writeback should be preserved when conflict copy also fails"
    );

    cleanup(&base);
}
