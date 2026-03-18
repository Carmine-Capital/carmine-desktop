//! Tests for VFS-path timeout wrapping on Graph API calls.
//!
//! Verifies that `CoreOps` enforces a timeout on all Graph API calls made from
//! VFS callback paths, and that timeout/network errors transition the VFS to
//! offline mode.

use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::{DriveItem, FolderFacet};
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::core_ops::CoreOps;
use carminedesktop_vfs::inode::InodeTable;
use std::ffi::OsStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const DRIVE_ID: &str = "drive-timeout-test";

fn test_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}

fn test_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "carminedesktop-timeout-vfs-{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(
        CacheManager::new(cache_dir, db_path, 100_000_000, Some(300), DRIVE_ID.to_string())
            .unwrap(),
    );
    (cache, base)
}

fn make_folder(id: &str, name: &str) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 0,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: None,
        folder: Some(FolderFacet { child_count: 0 }),
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    }
}

/// Test 1: When a Graph API call exceeds the timeout duration (5s),
/// the call completes in ~5s (not 10s), proving the timeout fires.
///
/// RED: Without timeout wrapping, the call takes the full 10s mock delay.
/// GREEN: With `graph_with_timeout`, the call returns in ~5s.
#[tokio::test(flavor = "multi_thread")]
async fn test_core_ops_find_child_returns_within_timeout_on_slow_server() {
    let server = MockServer::start().await;

    // Mock responds after 10 seconds — well beyond VFS_GRAPH_TIMEOUT (5s)
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "value": [] }))
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&server)
        .await;

    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("timeout-fast");
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

    let parent_ino = inodes.allocate("parent-folder");
    let parent_item = make_folder("parent-folder", "root");
    cache.memory.insert(parent_ino, parent_item);

    let ops2 = ops.clone();
    let start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        ops2.find_child(parent_ino, OsStr::new("nonexistent.txt"))
    })
    .await
    .unwrap();

    let elapsed = start.elapsed();

    // With VFS_GRAPH_TIMEOUT=5s, the call should return in ~5s, not 10s
    assert!(
        elapsed < Duration::from_secs(8),
        "find_child should return within ~5s timeout, not wait for full 10s response (took {elapsed:?})"
    );
    assert!(result.is_none(), "find_child should return None on timeout");

    let _ = std::fs::remove_dir_all(&base);
}

/// Test 2: When a Graph API call times out, the offline flag is set to true.
///
/// RED: Without timeout, the call completes normally after 10s; offline flag stays false.
/// GREEN: With `graph_with_timeout`, timeout triggers `set_offline()`.
#[tokio::test(flavor = "multi_thread")]
async fn test_core_ops_timeout_sets_offline_flag() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "value": [] }))
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&server)
        .await;

    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("timeout-offline");
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

    let parent_ino = inodes.allocate("parent-folder");
    let parent_item = make_folder("parent-folder", "root");
    cache.memory.insert(parent_ino, parent_item);

    let ops2 = ops.clone();
    let _ = tokio::task::spawn_blocking(move || ops2.list_children(parent_ino))
        .await
        .unwrap();

    assert!(
        offline.load(Ordering::Relaxed),
        "offline flag should be set after Graph API timeout"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Test 3: After the offline flag is set, subsequent calls skip Graph API entirely.
#[tokio::test(flavor = "multi_thread")]
async fn test_core_ops_offline_skips_graph_api() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("offline-skip");
    let inodes = Arc::new(InodeTable::new());

    // Start already offline
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

    let parent_ino = inodes.allocate("parent-folder");

    let ops2 = ops.clone();
    let children = tokio::task::spawn_blocking(move || ops2.list_children(parent_ino))
        .await
        .unwrap();

    assert!(children.is_empty());

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "offline mode should skip all Graph API calls"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Test 4: When not offline, Graph API calls that complete within timeout succeed normally.
#[tokio::test(flavor = "multi_thread")]
async fn test_core_ops_graph_call_within_timeout_succeeds() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "value": [{
                "id": "child-1",
                "name": "hello.txt",
                "size": 42,
                "file": { "mimeType": "text/plain" }
            }]
        })))
        .mount(&server)
        .await;

    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("within-timeout");
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

    let parent_ino = inodes.allocate("parent-folder");
    let parent_item = make_folder("parent-folder", "root");
    cache.memory.insert(parent_ino, parent_item);

    let ops2 = ops.clone();
    let result = tokio::task::spawn_blocking(move || {
        ops2.find_child(parent_ino, OsStr::new("hello.txt"))
    })
    .await
    .unwrap();

    assert!(result.is_some(), "find_child should find the child");
    let (_, item) = result.unwrap();
    assert_eq!(item.name, "hello.txt");
    assert!(
        !offline.load(Ordering::Relaxed),
        "offline flag should remain false"
    );

    let _ = std::fs::remove_dir_all(&base);
}
