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

use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder_file::PlaceholderFile;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_cache::CacheManager;
use cloudmount_graph::GraphClient;
use cloudmount_vfs::CfMountHandle;
use cloudmount_vfs::active_mount_count;
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
        let mount = CfMountHandle::mount(
            graph,
            cache,
            inodes,
            DRIVE_ID.to_string(),
            &mount_path,
            rt,
            test_id.to_string(),
            "Test Mount".to_string(),
            None,
        )
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

    /// Create placeholders directly via CfCreatePlaceholders.
    ///
    /// The OS `fetch_placeholders` callback does not fire reliably on
    /// Windows Server (CI runners), so tests must create placeholders
    /// explicitly instead of relying on directory enumeration to trigger it.
    fn create_root_placeholders(&self) {
        PlaceholderFile::new("hello.txt")
            .metadata(Metadata::file().size(13))
            .blob(b"file-1".to_vec())
            // No mark_in_sync(): file is intentionally dehydrated so the OS fires
            // fetch_data on access and accepts CfExecute(TRANSFER_DATA) writes.
            .create::<&std::path::Path>(self.mount_path.as_path())
            .expect("create hello.txt placeholder");

        PlaceholderFile::new("docs")
            .metadata(Metadata::directory())
            .blob(b"folder-1".to_vec())
            .mark_in_sync()
            .create::<&std::path::Path>(self.mount_path.as_path())
            .expect("create docs placeholder");
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
    fixture.create_root_placeholders();

    let entries: Vec<String> = std::fs::read_dir(&fixture.mount_path)
        .expect("read_dir failed")
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
    fixture.create_root_placeholders();

    // Reading the file triggers fetch_data callback (hydration).
    // Use spawn_blocking so tokio worker threads stay free to drive the I/O
    // for Handle::block_on() calls inside the CfApi callback.
    let p = fixture.path("hello.txt");
    let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(p))
        .await
        .unwrap()
        .expect("hydration read failed");
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
    fixture.create_root_placeholders();

    // Hydrate the file before editing.
    // Use spawn_blocking so tokio worker threads stay free to drive the I/O
    // for Handle::block_on() calls inside the CfApi callback.
    let p = fixture.path("hello.txt");
    let _ = tokio::task::spawn_blocking(move || std::fs::read_to_string(p))
        .await
        .unwrap();

    // Edit the file — CfApi detects local change via `closed` callback
    let p = fixture.path("hello.txt");
    tokio::task::spawn_blocking(move || std::fs::write(p, "New content"))
        .await
        .unwrap()
        .expect("write failed");

    // Give sync time to upload
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    fixture.teardown();
}

/// Copy-in scenario (Phase 7 task 7.1): a file written from outside the sync
/// root is detected by the watcher, `ingest_local_change()` calls
/// `register_local_file()` to create a `local:*` item, and
/// `stage_writeback_from_disk()` uploads it — NOT skipped by the unmodified
/// guard.  The `.expect(1)` on the upload mock verifies exactly one upload.
/// See also `cfapi_local_item_not_skipped_by_unmodified_guard` which isolates
/// the guard bypass for `local:*` item IDs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_external_copy_in_uploads_without_restart() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}:/copied-in.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "new-file-1",
            "copied-in.txt",
            12,
            false,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    let p = fixture.path("copied-in.txt");
    tokio::task::spawn_blocking(move || std::fs::write(p, b"hello copy-in"))
        .await
        .unwrap()
        .expect("copy-in write failed");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify the file exists locally after being ingested.
    assert!(
        std::fs::metadata(fixture.path("copied-in.txt")).is_ok(),
        "copied-in file should exist in sync root"
    );

    // The `.expect(1)` mock assertion fires during teardown, confirming the
    // upload was NOT skipped by the unmodified guard.
    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_safe_save_reconcile_keeps_final_remote_name() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;
    mock_file_download(&server, "file-1", b"Hello, world!").await;

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
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("PATCH"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "file-1",
            "hello.bak",
            13,
            false,
        )))
        .expect(0)
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;
    fixture.create_root_placeholders();

    let p = fixture.path("hello.txt");
    let _ = tokio::task::spawn_blocking(move || std::fs::read_to_string(p))
        .await
        .unwrap();

    std::fs::rename(fixture.path("hello.txt"), fixture.path("hello.bak"))
        .expect("backup rename failed");
    std::fs::write(fixture.path("~$hello.tmp"), "New content").expect("temp write failed");
    std::fs::rename(fixture.path("~$hello.tmp"), fixture.path("hello.txt"))
        .expect("replace rename failed");

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

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
    fixture.create_root_placeholders();

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
    fixture.create_root_placeholders();

    std::fs::remove_file(fixture.path("hello.txt")).expect("delete failed");

    // Give sync time to propagate
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    assert!(
        std::fs::metadata(fixture.path("hello.txt")).is_err(),
        "deleted file should not exist"
    );

    fixture.teardown();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_first_mount_registers_context_menu() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let initial_count = active_mount_count();
    let fixture = CfTestFixture::setup(&server).await;

    let new_count = active_mount_count();
    assert_eq!(
        new_count,
        initial_count + 1,
        "mount should increment active count"
    );

    fixture.teardown();

    let final_count = active_mount_count();
    assert_eq!(
        final_count, initial_count,
        "unmount should decrement active count"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_multi_mount_lifecycle() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    let initial_count = active_mount_count();

    let fixture1 = CfTestFixture::setup(&server).await;
    assert_eq!(
        active_mount_count(),
        initial_count + 1,
        "first mount should increment count"
    );

    let fixture2 = CfTestFixture::setup(&server).await;
    assert_eq!(
        active_mount_count(),
        initial_count + 2,
        "second mount should increment count"
    );

    fixture1.teardown();
    assert_eq!(
        active_mount_count(),
        initial_count + 1,
        "first unmount should decrement but not clear count"
    );

    fixture2.teardown();
    assert_eq!(
        active_mount_count(),
        initial_count,
        "final unmount should clear count"
    );
}

/// Verify that a `local:*` item is NOT skipped by the unmodified guard in
/// `stage_writeback_from_disk()` even when the file's mtime and size match the
/// cached DriveItem.
///
/// Before the fix (adding `!item_id.starts_with("local:")` to the guard),
/// `register_local_file()` created a DriveItem with the SAME mtime/size as the
/// local file, so the comparison always concluded "unmodified" and silently
/// dropped the upload.  This test writes a file into the sync root (triggering
/// `register_local_file` followed by `stage_writeback_from_disk`) and asserts
/// that the upload endpoint IS called exactly once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_local_item_not_skipped_by_unmodified_guard() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    // The upload mock expects exactly 1 request — if the unmodified guard
    // incorrectly skips the local:* item, this expectation will fail.
    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}:/local-new.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "server-assigned-id",
            "local-new.txt",
            17,
            false,
        )))
        .expect(1)
        .named("local item upload")
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    // Write a new file into the sync root.  The CfApi closed callback fires
    // `register_local_file()` (creating a DriveItem with matching mtime/size)
    // and then calls `stage_writeback_from_disk()`.  With the fix, the
    // unmodified guard is bypassed for `local:*` IDs, so the file is staged
    // and uploaded.
    let p = fixture.path("local-new.txt");
    tokio::task::spawn_blocking(move || std::fs::write(p, b"local file content"))
        .await
        .unwrap()
        .expect("local file write failed");

    // Allow time for the sync pipeline to detect and upload the file.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Teardown verifies the mock expectation (exactly 1 upload call).
    fixture.teardown();
}

/// Internal copy scenario (Phase 7 task 7.2): a placeholder file inside one
/// subfolder is copied to another subfolder within the same sync root.  The
/// copy creates a new file in the destination, which the watcher detects and
/// ingests via `register_local_file()` + `stage_writeback_from_disk()`.
/// The upload mock on the destination folder expects exactly one PUT.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_internal_copy_between_subfolders_triggers_upload() {
    let server = MockServer::start().await;

    // Root listing includes two subfolders.
    mock_root_item(&server).await;
    let children_path = format!("/drives/{DRIVE_ID}/items/{ROOT_ITEM_ID}/children");
    Mock::given(method("GET"))
        .and(path(&children_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                drive_item_json("folder-src", "src-folder", 0, true),
                drive_item_json("folder-dst", "dst-folder", 0, true),
            ]
        })))
        .mount(&server)
        .await;

    // Children listing for src-folder contains one file.
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/folder-src/children"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                drive_item_json("src-file-1", "data.txt", 18, false),
            ]
        })))
        .mount(&server)
        .await;

    // Children listing for dst-folder is initially empty.
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/folder-dst/children"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": []
        })))
        .mount(&server)
        .await;

    // Download mock for hydrating the source file before copy.
    mock_file_download(&server, "src-file-1", b"internal copy data").await;

    // The upload endpoint for the destination folder expects exactly 1 PUT.
    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/folder-dst:/data.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(drive_item_json(
            "dst-file-new",
            "data.txt",
            18,
            false,
        )))
        .expect(1)
        .named("internal copy upload to dst-folder")
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;

    // Create subfolder placeholders in the sync root.
    PlaceholderFile::new("src-folder")
        .metadata(Metadata::directory())
        .blob(b"folder-src".to_vec())
        .mark_in_sync()
        .create::<&std::path::Path>(fixture.mount_path.as_path())
        .expect("create src-folder placeholder");

    PlaceholderFile::new("dst-folder")
        .metadata(Metadata::directory())
        .blob(b"folder-dst".to_vec())
        .mark_in_sync()
        .create::<&std::path::Path>(fixture.mount_path.as_path())
        .expect("create dst-folder placeholder");

    // Create the source file placeholder inside src-folder.
    let src_dir = fixture.path("src-folder");
    PlaceholderFile::new("data.txt")
        .metadata(Metadata::file().size(18))
        .blob(b"src-file-1".to_vec())
        .create::<&std::path::Path>(src_dir.as_path())
        .expect("create data.txt placeholder in src-folder");

    // Hydrate the source file so we have content to copy.
    let src_path = fixture.path("src-folder").join("data.txt");
    let _ = tokio::task::spawn_blocking(move || std::fs::read(src_path))
        .await
        .unwrap()
        .expect("hydrate source file");

    // Copy the file to dst-folder.  This is the "internal copy" — the new
    // file at dst-folder/data.txt should be detected and uploaded.
    let src = fixture.path("src-folder").join("data.txt");
    let dst = fixture.path("dst-folder").join("data.txt");
    tokio::task::spawn_blocking(move || std::fs::copy(src, dst))
        .await
        .unwrap()
        .expect("internal copy failed");

    // Allow time for the watcher to detect and the pipeline to upload.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    assert!(
        std::fs::metadata(fixture.path("dst-folder").join("data.txt")).is_ok(),
        "copied file should exist in dst-folder"
    );

    // The `.expect(1)` mock assertion fires during teardown.
    fixture.teardown();
}

/// Rename with Graph API failure (Phase 7 task 7.3): renaming a placeholder
/// file triggers `core.rename()` which calls the Graph PATCH endpoint.  When
/// the server returns 500, the rename callback still calls `ticket.pass()` so
/// the OS sees a successful local rename.  The file should exist at the new
/// name after the operation.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows CfApi"]
async fn cfapi_rename_calls_ticket_pass_on_graph_failure() {
    let server = MockServer::start().await;
    mock_root_listing(&server).await;

    // Mock the PATCH endpoint to return 500 — simulating Graph API failure.
    Mock::given(method("PATCH"))
        .and(path(format!("/drives/{DRIVE_ID}/items/file-1")))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": {
                "code": "generalException",
                "message": "Internal Server Error"
            }
        })))
        .expect(1)
        .named("rename PATCH (should fail with 500)")
        .mount(&server)
        .await;

    let fixture = CfTestFixture::setup(&server).await;
    fixture.create_root_placeholders();

    // Rename the placeholder file.  CfApi fires the rename callback, which
    // calls core.rename() → Graph PATCH → 500 error.  The fix ensures
    // ticket.pass() is called in the Err branch, so the OS considers the
    // rename successful.
    let old_path = fixture.path("hello.txt");
    let new_path = fixture.path("hello-renamed.txt");
    let np = new_path.clone();
    tokio::task::spawn_blocking(move || std::fs::rename(old_path, np))
        .await
        .unwrap()
        .expect("local rename should succeed even when Graph API fails");

    // Give sync time to propagate the callback.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // The file should exist at the new name — ticket.pass() was called.
    assert!(
        std::fs::metadata(&new_path).is_ok(),
        "renamed file should exist at the new path"
    );

    // The original should no longer exist.
    assert!(
        std::fs::metadata(fixture.path("hello.txt")).is_err(),
        "original file should not exist after rename"
    );

    // The `.expect(1)` mock verifies the PATCH was attempted during teardown.
    fixture.teardown();
}
