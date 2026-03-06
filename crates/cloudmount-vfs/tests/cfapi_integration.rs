//! Cloud Files API integration tests — register sync root, populate placeholders, hydrate.
//!
//! Requirements:
//! - Windows 10 1709+ or Windows 11
//! - Runs as a normal user (no admin required for CfApi)
//!
//! Run with: `cargo test -p cloudmount-vfs --test cfapi_integration -- --ignored`

#![cfg(target_os = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_cache::CacheManager;
use cloudmount_graph::GraphClient;
use cloudmount_vfs::CfMountHandle;
use cloudmount_vfs::inode::InodeTable;

const DRIVE_ID: &str = "test-drive";
const ROOT_ITEM_ID: &str = "root-id";

fn drive_item_json(id: &str, name: &str, size: i64, is_folder: bool) -> serde_json::Value {
    let mut val = json!({
        "id": id,
        "name": name,
        "size": size,
        "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
    });
    if is_folder {
        val["folder"] = json!({ "childCount": 0 });
    } else {
        val["file"] = json!({ "mimeType": "application/octet-stream" });
    }
    val
}

struct CfTestFixture {
    mount: Option<CfMountHandle>,
    mount_path: PathBuf,
    _base_dir: PathBuf,
}

impl CfTestFixture {
    async fn setup(server: &MockServer) -> Self {
        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let base = std::env::temp_dir().join(format!("cloudmount-cfapi-test-{test_id}"));
        let cache_dir = base.join("cache");
        let db_path = base.join("metadata.db");
        let mount_path = base.join("sync");

        std::fs::create_dir_all(&cache_dir).unwrap();

        let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
            Ok("test-token".to_string())
        }));

        let cache =
            Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());

        let inodes = Arc::new(InodeTable::new());

        let rt = tokio::runtime::Handle::current();
        let mount =
            CfMountHandle::mount(graph, cache, inodes, DRIVE_ID.to_string(), &mount_path, rt)
                .expect("CfApi mount failed — is this Windows 10 1709+?");

        // Allow time for sync root to initialize
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        Self {
            mount: Some(mount),
            mount_path,
            _base_dir: base,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.mount_path.join(name)
    }

    fn teardown(mut self) {
        if let Some(m) = self.mount.take() {
            let _ = m.unmount();
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&self._base_dir);
    }
}

impl Drop for CfTestFixture {
    fn drop(&mut self) {
        if let Some(m) = self.mount.take() {
            let _ = m.unmount();
        }
    }
}

async fn mock_root_item(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/root")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": ROOT_ITEM_ID,
            "name": "root",
            "size": 0,
            "folder": { "childCount": 0 },
        })))
        .mount(server)
        .await;
}

async fn mock_root_listing(server: &MockServer) {
    mock_root_item(server).await;
    let children_path = format!("/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}/children");

    Mock::given(method("GET"))
        .and(path(&children_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                drive_item_json("file-1", "hello.txt", 13, false),
                drive_item_json("folder-1", "docs", 0, true),
            ]
        })))
        .mount(server)
        .await;
}

async fn mock_file_download(server: &MockServer, item_id: &str, content: &[u8]) {
    let dl_path = format!("/drives/{DRIVE_ID}/items/{item_id}/content");

    Mock::given(method("GET"))
        .and(path(&dl_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(content.to_vec(), "application/octet-stream"),
        )
        .mount(server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_mount_and_unmount_lifecycle() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = CfTestFixture::setup(&server).await;
    assert!(
        fixture.mount_path.exists(),
        "sync root directory should exist"
    );
    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_browse_populates_placeholders() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = CfTestFixture::setup(&server).await;

    // Browsing the sync root triggers fetch_placeholders callback
    let entries: Vec<String> = std::fs::read_dir(&fixture.mount_path)
        .expect("read_dir on sync root failed")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        entries.contains(&"hello.txt".to_string()),
        "expected hello.txt in placeholders, got: {entries:?}"
    );
    assert!(
        entries.contains(&"docs".to_string()),
        "expected docs folder in placeholders, got: {entries:?}"
    );

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_hydrate_file_on_read() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;
    mock_file_download(&server, "file-1", b"Hello, world!").await;

    let fixture = CfTestFixture::setup(&server).await;

    // Browse to trigger placeholder creation
    let _ = std::fs::read_dir(&fixture.mount_path);

    // Allow placeholders to populate
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Reading the file triggers fetch_data callback (hydration)
    let content =
        std::fs::read_to_string(fixture.path("hello.txt")).expect("hydration read failed");
    assert_eq!(content, "Hello, world!");

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_edit_and_sync_file() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;
    mock_file_download(&server, "file-1", b"Hello, world!").await;

    // Mock the upload endpoint
    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}:/hello.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "file-1",
            "hello.txt",
            11,
            false,
        )))
        .mount(&server)
        .await;

    // Mock get_item for conflict check
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "file-1",
            "hello.txt",
            13,
            false,
        )))
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    // Browse + hydrate
    let _ = std::fs::read_dir(&fixture.mount_path);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let _ = std::fs::read_to_string(fixture.path("hello.txt"));

    // Edit the file — CfApi detects local change via `closed` callback
    std::fs::write(fixture.path("hello.txt"), "New content").expect("write failed");

    // Give sync time to upload
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_rename_file() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    // Mock the rename (PATCH) endpoint
    Mock::given(method("PATCH"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "file-1",
            "renamed.txt",
            13,
            false,
        )))
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mount_path);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    std::fs::rename(fixture.path("hello.txt"), fixture.path("renamed.txt")).expect("rename failed");

    // Give sync time to propagate
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    assert!(
        std::fs::metadata(fixture.path("renamed.txt")).is_ok(),
        "renamed file should exist"
    );

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_delete_file() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("DELETE"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mount_path);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    std::fs::remove_file(fixture.path("hello.txt")).expect("delete failed");

    // Give sync time to propagate
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    assert!(
        std::fs::metadata(fixture.path("hello.txt")).is_err(),
        "deleted file should not exist"
    );

    fixture.teardown();
}
