use std::sync::Arc;

use carminedesktop_cache::{CacheManager, OfflineManager, PinResult};
use carminedesktop_core::types::{DriveItem, FileFacet, FolderFacet, ParentReference};
use wiremock::MockServer;

/// Create a test CacheManager with unique temp paths.
fn test_cache() -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "carminedesktop-offline-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300), "test-drive".to_string()).unwrap());
    (cache, base)
}

fn test_graph(server: &MockServer) -> Arc<carminedesktop_graph::GraphClient> {
    Arc::new(carminedesktop_graph::GraphClient::with_base_url(
        server.uri(),
        || async { Ok("test-token".to_string()) },
    ))
}

fn make_folder(id: &str, name: &str, size: i64) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: Some(ParentReference {
            drive_id: Some("test-drive".to_string()),
            id: Some("root".to_string()),
            path: None,
        }),
        folder: Some(FolderFacet { child_count: 2 }),
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    }
}

fn _make_file(id: &str, name: &str) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 1024,
        last_modified: None,
        created: None,
        etag: Some(format!("etag-{id}")),
        parent_reference: Some(ParentReference {
            drive_id: Some("test-drive".to_string()),
            id: Some("root".to_string()),
            path: None,
        }),
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

#[tokio::test]
async fn test_offline_pin_folder_success() {
    let server = MockServer::start().await;
    let (cache, base) = test_cache();
    let graph = test_graph(&server);
    let drive_id = "test-drive";

    let _folder = make_folder("folder-1", "Documents", 1_000_000);

    // Mock get_item for the folder
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/items/folder-1"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "folder-1",
                "name": "Documents",
                "size": 1_000_000,
                "folder": { "childCount": 0 }
            })),
        )
        .mount(&server)
        .await;

    // Mock list_children (empty folder for simplicity)
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/items/folder-1/children"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": []
            })),
        )
        .mount(&server)
        .await;

    let mgr = OfflineManager::new(
        cache.pin_store.clone(),
        graph,
        cache.clone(),
        drive_id.to_string(),
        86400,         // 1 day TTL
        5_000_000_000, // 5 GB max
    );

    let result = mgr.pin_folder("folder-1", "Documents").await.unwrap();
    assert!(matches!(result, PinResult::Ok));

    // Verify pin record exists
    assert!(cache.pin_store.is_pinned(drive_id, "folder-1"));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_offline_pin_folder_too_large() {
    let server = MockServer::start().await;
    let (cache, base) = test_cache();
    let graph = test_graph(&server);
    let drive_id = "test-drive";

    // Mock get_item for a large folder
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/items/big-folder"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "big-folder",
                "name": "BigFolder",
                "size": 10_000_000_000_i64,
                "folder": { "childCount": 1000 }
            })),
        )
        .mount(&server)
        .await;

    let mgr = OfflineManager::new(
        cache.pin_store.clone(),
        graph,
        cache.clone(),
        drive_id.to_string(),
        86400,
        5_000_000_000, // 5 GB max
    );

    let result = mgr.pin_folder("big-folder", "BigFolder").await.unwrap();
    assert!(matches!(result, PinResult::Rejected { .. }));

    // No pin record should exist
    assert!(!cache.pin_store.is_pinned(drive_id, "big-folder"));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_offline_pin_file_rejected() {
    let server = MockServer::start().await;
    let (cache, base) = test_cache();
    let graph = test_graph(&server);
    let drive_id = "test-drive";

    // Mock get_item for a file (not a folder)
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/items/file-1"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "file-1",
                "name": "report.pdf",
                "size": 1024,
                "file": { "mimeType": "application/pdf" }
            })),
        )
        .mount(&server)
        .await;

    let mgr = OfflineManager::new(
        cache.pin_store.clone(),
        graph,
        cache.clone(),
        drive_id.to_string(),
        86400,
        5_000_000_000,
    );

    let result = mgr.pin_folder("file-1", "report.pdf").await.unwrap();
    assert!(matches!(result, PinResult::Rejected { .. }));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_offline_unpin_folder() {
    let server = MockServer::start().await;
    let (cache, base) = test_cache();
    let graph = test_graph(&server);
    let drive_id = "test-drive";

    // Pin directly via PinStore
    cache.pin_store.pin(drive_id, "folder-1", 86400).unwrap();
    assert!(cache.pin_store.is_pinned(drive_id, "folder-1"));

    let mgr = OfflineManager::new(
        cache.pin_store.clone(),
        graph,
        cache.clone(),
        drive_id.to_string(),
        86400,
        5_000_000_000,
    );

    mgr.unpin_folder("folder-1").unwrap();
    assert!(!cache.pin_store.is_pinned(drive_id, "folder-1"));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_offline_process_expired() {
    let server = MockServer::start().await;
    let (cache, base) = test_cache();
    let graph = test_graph(&server);
    let drive_id = "test-drive";

    // Pin with TTL=0 (expires immediately)
    cache.pin_store.pin(drive_id, "folder-1", 0).unwrap();
    // Wait for expiry
    std::thread::sleep(std::time::Duration::from_millis(200));

    let mgr = OfflineManager::new(
        cache.pin_store.clone(),
        graph,
        cache.clone(),
        drive_id.to_string(),
        86400,
        5_000_000_000,
    );

    let expired = mgr.process_expired().unwrap();
    assert!(!expired.is_empty(), "should have expired pins");

    // Pin record should be removed
    assert!(!cache.pin_store.is_pinned(drive_id, "folder-1"));

    let _ = std::fs::remove_dir_all(&base);
}
