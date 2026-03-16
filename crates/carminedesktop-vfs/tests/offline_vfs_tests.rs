//! Tests for VFS offline (cache-only) mode.

use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::{DriveItem, FileFacet};
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::core_ops::CoreOps;
use carminedesktop_vfs::inode::InodeTable;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use wiremock::MockServer;

const DRIVE_ID: &str = "drive-offline-test";

fn test_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}

fn test_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "carminedesktop-offline-vfs-{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    (cache, base)
}

fn make_file(id: &str, name: &str, size: i64, etag: &str) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size,
        last_modified: None,
        created: None,
        etag: Some(etag.to_string()),
        parent_reference: None,
        folder: None,
        file: Some(FileFacet {
            mime_type: Some("text/plain".to_string()),
            hashes: None,
        }),
        publication: None,
        download_url: None,
        web_url: None,
    }
}

/// list_children in offline mode returns SQLite-cached children without calling Graph API.
#[tokio::test]
async fn list_children_returns_sqlite_data_when_offline() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("list-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(
        graph,
        cache.clone(),
        inodes.clone(),
        DRIVE_ID.to_string(),
        rt,
    )
    .with_offline_flag(offline);

    let parent_ino = inodes.allocate("parent-folder");
    let child = make_file("child-file", "readme.txt", 100, "etag1");
    let child_ino = inodes.allocate(&child.id);
    cache
        .sqlite
        .upsert_item(child_ino, DRIVE_ID, &child, Some(parent_ino))
        .unwrap();

    // list_children is sync (SQLite path does not call block_on) so safe to call directly
    let children = ops.list_children(parent_ino);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].1.name, "readme.txt");

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "offline mode should not make Graph API calls"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// open_file in offline mode serves content from disk cache without calling Graph API.
#[tokio::test(flavor = "multi_thread")]
async fn open_file_serves_disk_cache_when_offline() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("open-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = Arc::new(
        CoreOps::new(
            graph,
            cache.clone(),
            inodes.clone(),
            DRIVE_ID.to_string(),
            rt,
        )
        .with_offline_flag(offline),
    );

    let file = make_file("file1", "doc.txt", 5, "etag-abc");
    let file_ino = inodes.allocate(&file.id);
    cache.memory.insert(file_ino, file.clone());
    cache
        .disk
        .put(DRIVE_ID, &file.id, b"hello", Some("etag-abc"))
        .await
        .unwrap();

    // open_file calls rt.block_on internally — must run outside async context
    let ops2 = ops.clone();
    let fh = tokio::task::spawn_blocking(move || ops2.open_file(file_ino))
        .await
        .unwrap();
    assert!(
        fh.is_ok(),
        "open_file should succeed offline with cached content"
    );

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "offline mode should not make Graph API calls"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// read_content in offline mode serves content from disk cache without calling Graph API.
#[tokio::test(flavor = "multi_thread")]
async fn read_content_serves_disk_cache_when_offline() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("read-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = Arc::new(
        CoreOps::new(
            graph,
            cache.clone(),
            inodes.clone(),
            DRIVE_ID.to_string(),
            rt,
        )
        .with_offline_flag(offline),
    );

    let file = make_file("file2", "data.bin", 11, "etag-xyz");
    let file_ino = inodes.allocate(&file.id);
    cache.memory.insert(file_ino, file.clone());
    cache
        .disk
        .put(DRIVE_ID, &file.id, b"hello world", Some("etag-xyz"))
        .await
        .unwrap();

    // read_content calls rt.block_on internally — must run outside async context
    let ops2 = ops.clone();
    let content = tokio::task::spawn_blocking(move || ops2.read_content(file_ino))
        .await
        .unwrap();
    assert!(content.is_ok());
    assert_eq!(content.unwrap(), b"hello world");

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "offline mode should not make Graph API calls"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// A network error during list_children sets the offline flag for future operations.
#[tokio::test(flavor = "multi_thread")]
async fn network_error_sets_offline_flag() {
    // Use a port that is guaranteed to refuse connections (port 1 is never open on localhost).
    // The with_retry back-offs add ~7 s; accepted cost for verifying the Network error path.
    let graph = test_graph("http://127.0.0.1:1");
    let (cache, base) = test_cache("net-error");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(false));

    let rt = tokio::runtime::Handle::current();
    let ops = Arc::new(
        CoreOps::new(
            graph,
            cache.clone(),
            inodes.clone(),
            DRIVE_ID.to_string(),
            rt,
        )
        .with_offline_flag(offline.clone()),
    );

    // Allocate a parent with no cached children to force a Graph API call
    let parent_ino = inodes.allocate("empty-parent");

    // list_children calls rt.block_on for the Graph API fallback — must run outside async context
    let ops2 = ops.clone();
    let _ = tokio::task::spawn_blocking(move || ops2.list_children(parent_ino))
        .await
        .unwrap();

    assert!(
        offline.load(Ordering::Relaxed),
        "offline flag should be set after network error"
    );

    let _ = std::fs::remove_dir_all(&base);
}
