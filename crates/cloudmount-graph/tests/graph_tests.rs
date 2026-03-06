use bytes::Bytes;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_graph::GraphClient;

fn make_client(base_url: &str) -> GraphClient {
    GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    })
}

fn drive_item_json(id: &str, name: &str, size: i64) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "size": size,
    })
}

#[tokio::test]
async fn get_my_drive_returns_drive() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "drive-123",
            "name": "OneDrive",
            "driveType": "personal",
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let drive = client.get_my_drive().await.unwrap();

    assert_eq!(drive.id, "drive-123");
    assert_eq!(drive.name, "OneDrive");
    assert_eq!(drive.drive_type.as_deref(), Some("personal"));
}

#[tokio::test]
async fn list_children_paginates_two_pages() {
    let server = MockServer::start().await;
    let page2_url = format!("{}/page2", server.uri());

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/root/children"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [drive_item_json("item1", "file1.txt", 100)],
            "@odata.nextLink": page2_url,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [drive_item_json("item2", "file2.txt", 200)],
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let items = client.list_children("d1", "root").await.unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "item1");
    assert_eq!(items[0].name, "file1.txt");
    assert_eq!(items[1].id, "item2");
    assert_eq!(items[1].name, "file2.txt");
}

#[tokio::test]
async fn download_content_returns_bytes() {
    let server = MockServer::start().await;
    let payload = b"binary-file-content-0xDEADBEEF";

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/i1/content"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(payload.to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let data = client.download_content("d1", "i1").await.unwrap();

    assert_eq!(data.as_ref(), payload);
}

#[tokio::test]
async fn upload_small_returns_drive_item() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/drives/d1/items/p1:/test.txt:/content"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(drive_item_json("new-id", "test.txt", 42)),
        )
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let item = client
        .upload_small("d1", "p1", "test.txt", Bytes::from_static(b"hello"))
        .await
        .unwrap();

    assert_eq!(item.id, "new-id");
    assert_eq!(item.name, "test.txt");
    assert_eq!(item.size, 42);
}

#[tokio::test]
async fn create_folder_returns_drive_item() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/drives/d1/items/p1/children"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "folder-id",
            "name": "new-folder",
            "size": 0,
            "folder": { "childCount": 0 },
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let item = client
        .create_folder("d1", "p1", "new-folder")
        .await
        .unwrap();

    assert_eq!(item.id, "folder-id");
    assert_eq!(item.name, "new-folder");
    assert!(item.is_folder());
}

#[tokio::test]
async fn delete_item_succeeds_on_204() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/drives/d1/items/i1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let result = client.delete_item("d1", "i1").await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn update_item_returns_renamed_item() {
    let server = MockServer::start().await;

    Mock::given(method("PATCH"))
        .and(path("/drives/d1/items/i1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "i1",
            "renamed.txt",
            512,
        )))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let item = client
        .update_item("d1", "i1", Some("renamed.txt"), None)
        .await
        .unwrap();

    assert_eq!(item.id, "i1");
    assert_eq!(item.name, "renamed.txt");
}

#[tokio::test]
async fn delta_query_returns_items_and_delta_link() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/drives/d1/root/delta"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                drive_item_json("delta1", "changed.txt", 50),
                drive_item_json("delta2", "new.txt", 75),
            ],
            "@odata.deltaLink": "https://graph.microsoft.com/delta-token-abc",
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let resp = client.delta_query("d1", None).await.unwrap();

    assert_eq!(resp.value.len(), 2);
    assert_eq!(resp.value[0].name, "changed.txt");
    assert_eq!(resp.value[1].name, "new.txt");
    assert_eq!(
        resp.delta_link.as_deref(),
        Some("https://graph.microsoft.com/delta-token-abc")
    );
}

#[tokio::test]
async fn search_sites_returns_results() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/sites"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [{
                "id": "site-1",
                "displayName": "Engineering",
                "webUrl": "https://contoso.sharepoint.com/sites/engineering",
                "name": "engineering",
            }],
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let sites = client.search_sites("engineering").await.unwrap();

    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].id, "site-1");
    assert_eq!(sites[0].display_name.as_deref(), Some("Engineering"));
}

#[tokio::test]
async fn get_item_root_alias_hits_correct_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/root"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "actual-root-id",
            "name": "root",
            "size": 0,
            "folder": { "childCount": 5 },
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let item = client.get_item("d1", "root").await.unwrap();

    assert_eq!(item.id, "actual-root-id");
    assert!(item.is_folder());
}

#[tokio::test]
async fn error_404_returns_graph_api_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {
                "code": "itemNotFound",
                "message": "The resource could not be found.",
            }
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client.get_my_drive().await.unwrap_err();

    match err {
        cloudmount_core::Error::GraphApi { status, message } => {
            assert_eq!(status, 404);
            assert!(message.contains("itemNotFound"));
        }
        other => panic!("expected GraphApi error, got: {other:?}"),
    }
}

#[tokio::test]
async fn error_429_retries_then_fails() {
    tokio::time::pause();
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .expect(4) // 1 initial + 3 retries
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client.get_my_drive().await.unwrap_err();

    match err {
        cloudmount_core::Error::GraphApi { status, .. } => assert_eq!(status, 429),
        other => panic!("expected GraphApi 429 error, got: {other:?}"),
    }
}

#[tokio::test]
async fn error_500_retries_then_fails() {
    tokio::time::pause();
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": {
                "code": "internalServerError",
                "message": "An internal server error occurred.",
            }
        })))
        .expect(4) // 1 initial + 3 retries
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client.get_my_drive().await.unwrap_err();

    match err {
        cloudmount_core::Error::GraphApi { status, message } => {
            assert_eq!(status, 500);
            assert!(message.contains("internalServerError"));
        }
        other => panic!("expected GraphApi 500 error, got: {other:?}"),
    }
}
