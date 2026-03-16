//! FUSE integration tests — mount a real filesystem backed by wiremock.
//!
//! Requirements:
//! - Linux: `fusermount3` installed, user in `fuse` group
//! - macOS: macFUSE or FUSE-T installed
//!
//! Run with: `cargo test -p carminedesktop-vfs --test fuse_integration -- --ignored`

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use carminedesktop_cache::CacheManager;
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::MountHandle;
use carminedesktop_vfs::inode::InodeTable;

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

struct TestFixture {
    mount: Option<MountHandle>,
    mountpoint: PathBuf,
    _base_dir: PathBuf,
}

impl TestFixture {
    async fn setup(server: &MockServer) -> Self {
        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let base = std::env::temp_dir().join(format!("carminedesktop-vfs-test-{test_id}"));
        let cache_dir = base.join("cache");
        let db_path = base.join("metadata.db");
        let mountpoint = base.join("mnt");

        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::create_dir_all(&mountpoint).unwrap();

        let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
            Ok("test-token".to_string())
        }));

        let cache =
            Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());

        let inodes = Arc::new(InodeTable::new());

        let rt = tokio::runtime::Handle::current();
        let mount = MountHandle::mount(
            graph,
            cache,
            inodes,
            DRIVE_ID.to_string(),
            mountpoint.to_str().unwrap(),
            rt,
            None,
            None,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("FUSE mount failed — is FUSE available?");

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        Self {
            mount: Some(mount),
            mountpoint,
            _base_dir: base,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.mountpoint.join(name)
    }

    fn teardown(mut self) {
        if let Some(m) = self.mount.take() {
            let _ = m.unmount();
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = std::fs::remove_dir_all(&self._base_dir);
    }
}

impl Drop for TestFixture {
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
#[ignore = "requires FUSE"]
async fn mount_initializes_root_inode_from_graph() {
    let server = MockServer::start().await;
    mock_root_item(&server).await;

    let fixture = TestFixture::setup(&server).await;

    // getattr on the mountpoint root should succeed (ROOT_INODE is initialized)
    let meta = std::fs::metadata(&fixture.mountpoint).expect("getattr on root inode failed");
    assert!(meta.is_dir());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn mount_fails_when_root_fetch_returns_error() {
    let server = MockServer::start().await;

    // Mock root fetch to return 500
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/root")))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": { "code": "internalServerError", "message": "server error" }
        })))
        .mount(&server)
        .await;

    let base = std::env::temp_dir().join(format!(
        "carminedesktop-vfs-fail-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    let mountpoint = base.join("mnt");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&mountpoint).unwrap();

    let graph = Arc::new(carminedesktop_graph::GraphClient::with_base_url(
        server.uri(),
        || async { Ok("test-token".to_string()) },
    ));
    let cache = Arc::new(
        carminedesktop_cache::CacheManager::new(cache_dir, db_path, 100_000_000, Some(300))
            .unwrap(),
    );
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    let result = carminedesktop_vfs::MountHandle::mount(
        graph,
        cache,
        inodes,
        DRIVE_ID.to_string(),
        mountpoint.to_str().unwrap(),
        rt,
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    assert!(result.is_err(), "mount should fail when root fetch fails");
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn mount_and_unmount_lifecycle() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = TestFixture::setup(&server).await;
    assert!(fixture.mountpoint.exists());
    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn readdir_lists_root_contents() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = TestFixture::setup(&server).await;

    let entries: Vec<String> = std::fs::read_dir(&fixture.mountpoint)
        .expect("read_dir failed")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        entries.contains(&"hello.txt".to_string()),
        "entries: {entries:?}"
    );
    assert!(
        entries.contains(&"docs".to_string()),
        "entries: {entries:?}"
    );
    assert_eq!(
        entries.len(),
        2,
        "expected exactly 2 entries, got: {entries:?}"
    );

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn getattr_file_returns_correct_metadata() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    let meta = std::fs::metadata(fixture.path("hello.txt")).expect("metadata failed");
    assert!(meta.is_file());
    assert_eq!(meta.len(), 13);

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn getattr_folder_is_directory() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    let meta = std::fs::metadata(fixture.path("docs")).expect("metadata failed");
    assert!(meta.is_dir());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn read_file_returns_content() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;
    mock_file_download(&server, "file-1", b"Hello, world!").await;

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    let content = std::fs::read_to_string(fixture.path("hello.txt")).expect("read failed");
    assert_eq!(content, "Hello, world!");

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn write_and_flush_file() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;
    mock_file_download(&server, "file-1", b"Hello, world!").await;

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

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);
    std::fs::write(fixture.path("hello.txt"), "New content").expect("write failed");

    let content = std::fs::read_to_string(fixture.path("hello.txt")).expect("read after write");
    assert_eq!(content, "New content");

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn create_new_file() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}:/newfile.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "new-file-id",
            "newfile.txt",
            4,
            false,
        )))
        .mount(&server)
        .await;

    let fixture = TestFixture::setup(&server).await;

    std::fs::write(fixture.path("newfile.txt"), "test").expect("create failed");

    let meta = std::fs::metadata(fixture.path("newfile.txt")).expect("metadata after create");
    assert!(meta.is_file());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn mkdir_creates_folder_via_api() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("POST"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}/children"
        )))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "new-folder-id",
            "name": "new-dir",
            "size": 0,
            "folder": { "childCount": 0 },
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
        })))
        .mount(&server)
        .await;

    let fixture = TestFixture::setup(&server).await;

    std::fs::create_dir(fixture.path("new-dir")).expect("mkdir failed");
    let meta = std::fs::metadata(fixture.path("new-dir")).expect("metadata after mkdir");
    assert!(meta.is_dir());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn unlink_deletes_file_via_api() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("DELETE"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    std::fs::remove_file(fixture.path("hello.txt")).expect("unlink failed");
    assert!(std::fs::metadata(fixture.path("hello.txt")).is_err());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn rmdir_on_empty_folder() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/folder-1/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": []
        })))
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path(format!("/drives/{DRIVE_ID}/items/folder-1")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    std::fs::remove_dir(fixture.path("docs")).expect("rmdir failed");
    assert!(std::fs::metadata(fixture.path("docs")).is_err());

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn rename_file_same_directory() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

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

    let fixture = TestFixture::setup(&server).await;

    let _ = std::fs::read_dir(&fixture.mountpoint);

    std::fs::rename(fixture.path("hello.txt"), fixture.path("renamed.txt")).expect("rename failed");

    assert!(std::fs::metadata(fixture.path("hello.txt")).is_err());
    let meta = std::fs::metadata(fixture.path("renamed.txt")).expect("metadata after rename");
    assert!(meta.is_file());

    fixture.teardown();
}
