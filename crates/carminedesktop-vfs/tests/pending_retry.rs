use std::sync::Arc;

use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::{DriveItem, FileFacet, ParentReference};
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::retry_pending_writes_for_drive;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_pending_retry_recovers_after_transient_failure() {
    let server = MockServer::start().await;
    let drive_id = "test-drive";
    let parent_id = "root-id";
    let item_id = "file-1";

    let test_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("carminedesktop-pending-retry-{test_id}"));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = CacheManager::new(cache_dir, db_path, 100_000_000, Some(300), "test-drive".to_string()).unwrap();
    let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
        Ok("test-token".to_string())
    }));

    let item = DriveItem {
        id: item_id.to_string(),
        name: "hello.txt".to_string(),
        size: 5,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: Some(ParentReference {
            drive_id: Some(drive_id.to_string()),
            id: Some(parent_id.to_string()),
            path: None,
        }),
        folder: None,
        file: Some(FileFacet {
            mime_type: None,
            hashes: None,
        }),
        publication: None,
        download_url: None,
        web_url: None,
    };
    cache
        .sqlite
        .upsert_item(42, drive_id, &item, Some(1))
        .unwrap();
    cache
        .writeback
        .write(drive_id, item_id, b"hello")
        .await
        .unwrap();

    let upload_path = format!("/drives/{drive_id}/items/{parent_id}:/hello.txt:/content");
    Mock::given(method("PUT"))
        .and(path(&upload_path))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "error": {"code": "serviceUnavailable", "message": "retry later"}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path(&upload_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": item_id,
            "name": "hello.txt",
            "size": 5,
            "parentReference": { "driveId": drive_id, "id": parent_id },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    let uploaded_first =
        retry_pending_writes_for_drive(&cache, graph.as_ref(), drive_id, "test retry").await;
    assert_eq!(uploaded_first, 0);
    assert!(cache.writeback.has_pending(drive_id, item_id));

    let uploaded_second =
        retry_pending_writes_for_drive(&cache, graph.as_ref(), drive_id, "test retry").await;
    assert_eq!(uploaded_second, 1);
    assert!(!cache.writeback.has_pending(drive_id, item_id));

    let _ = std::fs::remove_dir_all(base);
}
