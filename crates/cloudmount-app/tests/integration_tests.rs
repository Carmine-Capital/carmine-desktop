use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use serde_json::json;
use tokio::time::sleep;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_cache::CacheManager;
use cloudmount_cache::disk::DiskCache;
use cloudmount_cache::writeback::WriteBackBuffer;
use cloudmount_core::config::{EffectiveConfig, UserConfig};
use cloudmount_core::types::{DriveItem, FolderFacet};
use cloudmount_graph::GraphClient;

// ============================================================================
// HELPERS
// ============================================================================

fn make_client(base_url: &str) -> GraphClient {
    GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    })
}

fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 1024,
        last_modified: None,
        created: None,
        etag: Some(format!("etag-{id}")),
        parent_reference: None,
        folder: if is_folder {
            Some(FolderFacet { child_count: 0 })
        } else {
            None
        },
        file: None,
        download_url: None,
    }
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let id = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("cloudmount-integration-{prefix}-{id}"))
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
}

// ============================================================================
// 11.1 — E2E ONEDRIVE (requires live Graph API)
// ============================================================================

#[tokio::test]
#[ignore = "requires live Graph API"]
async fn test_e2e_onedrive_mount_list_read_write_unmount() -> cloudmount_core::Result<()> {
    // 1. Authenticate via OAuth2 PKCE (real browser flow)
    // 2. Get user's OneDrive via GET /me/drive
    // 3. Mount drive via MountHandle::mount
    // 4. List root directory via std::fs::read_dir on mountpoint
    // 5. Read a file via std::fs::read
    // 6. Write a new file via std::fs::write
    // 7. Verify the file appears in directory listing
    // 8. Verify the file is uploaded to Graph API via get_item
    // 9. Unmount and verify mountpoint is empty
    todo!("implement with live Graph API credentials")
}

// ============================================================================
// 11.2 — E2E SHAREPOINT (requires live Graph API)
// ============================================================================

#[tokio::test]
#[ignore = "requires live Graph API"]
async fn test_e2e_sharepoint_browse_mount_read_unmount() -> cloudmount_core::Result<()> {
    // 1. Authenticate via OAuth2 PKCE
    // 2. Search sites via search_sites("*")
    // 3. List document libraries for first site via list_site_drives
    // 4. Mount a document library via MountHandle::mount
    // 5. Read root directory listing
    // 6. Read a file from the library
    // 7. Unmount cleanly
    todo!("implement with live Graph API credentials")
}

// ============================================================================
// 11.3 — OFFLINE BEHAVIOR (cached files readable without network)
// ============================================================================

#[tokio::test]
async fn test_offline_cached_files_readable() -> cloudmount_core::Result<()> {
    let base = unique_temp_dir("offline");
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = CacheManager::new(cache_dir, db_path, 100_000_000, Some(300))?;

    let item1 = test_drive_item("item-a", "report.docx", false);
    let item2 = test_drive_item("item-b", "notes.txt", false);
    cache.sqlite.upsert_item(10, "drive1", &item1, None)?;
    cache.sqlite.upsert_item(11, "drive1", &item2, None)?;
    cache
        .disk
        .put("drive1", "item-a", b"report-content-binary", None)
        .await?;
    cache
        .disk
        .put("drive1", "item-b", b"notes go here", None)
        .await?;

    cache.memory.insert(10, item1.clone());
    cache.memory.insert(11, item2.clone());

    // "Offline" — no MockServer, no network. Read purely from cache.
    let mem_item = cache.memory.get(10);
    assert!(mem_item.is_some());
    assert_eq!(mem_item.unwrap().name, "report.docx");

    let disk_a = cache.disk.get("drive1", "item-a").await;
    assert!(disk_a.is_some());
    assert_eq!(disk_a.unwrap(), b"report-content-binary");

    let disk_b = cache.disk.get("drive1", "item-b").await;
    assert!(disk_b.is_some());
    assert_eq!(disk_b.unwrap(), b"notes go here");

    let (inode, sqlite_item) = cache.sqlite.get_item_by_id("item-a")?.unwrap();
    assert_eq!(inode, 10);
    assert_eq!(sqlite_item.name, "report.docx");

    // Verify writeback buffer also works offline
    cache
        .writeback
        .write("drive1", "item-c", b"pending-offline-write")
        .await?;
    let pending = cache.writeback.read("drive1", "item-c").await;
    assert_eq!(pending.unwrap(), b"pending-offline-write");

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 11.4 — WRITE CONFLICT DETECTION
// ============================================================================

#[tokio::test]
async fn test_write_conflict_creates_conflict_copy() -> cloudmount_core::Result<()> {
    let server = MockServer::start().await;
    let base = unique_temp_dir("conflict");
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    let client = Arc::new(make_client(&server.uri()));
    let cache = CacheManager::new(cache_dir, db_path, 100_000_000, Some(300))?;

    // Populate cache with a file that has eTag "etag-v1"
    let mut item = test_drive_item("file-1", "document.txt", false);
    item.etag = Some("etag-v1".to_string());
    item.parent_reference = Some(cloudmount_core::types::ParentReference {
        drive_id: Some("drive1".to_string()),
        id: Some("parent-root".to_string()),
        path: None,
    });
    cache.sqlite.upsert_item(10, "drive1", &item, None)?;

    // Write pending local content
    cache
        .writeback
        .write("drive1", "file-1", b"my local changes")
        .await?;

    // Server returns a DIFFERENT eTag — simulating concurrent remote edit
    Mock::given(method("GET"))
        .and(path("/drives/drive1/items/file-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "file-1",
            "name": "document.txt",
            "size": 500,
            "eTag": "etag-v2-server-changed",
        })))
        .mount(&server)
        .await;

    let server_item = client.get_item("drive1", "file-1").await?;
    let cached_etag = item.etag.as_deref().unwrap();
    let conflict = server_item.etag.as_deref() != Some(cached_etag);
    assert!(conflict, "eTag mismatch should indicate a conflict");

    Mock::given(method("PUT"))
        .and(path(
            "/drives/drive1/items/parent-root:/document.txt.conflict:/content",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "conflict-copy-id",
            "name": "document.txt.conflict",
            "size": 16,
        })))
        .mount(&server)
        .await;

    if conflict {
        let conflict_name = format!("{}.conflict", item.name);
        let pending = cache.writeback.read("drive1", "file-1").await.unwrap();
        let conflict_item = client
            .upload_small(
                "drive1",
                "parent-root",
                &conflict_name,
                Bytes::from(pending),
            )
            .await?;
        assert_eq!(conflict_item.name, "document.txt.conflict");
    }

    // Original upload proceeds regardless of conflict
    Mock::given(method("PUT"))
        .and(path(
            "/drives/drive1/items/parent-root:/document.txt:/content",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "file-1",
            "name": "document.txt",
            "size": 16,
            "eTag": "etag-v3-after-upload",
        })))
        .mount(&server)
        .await;

    let upload_result = client
        .upload_small(
            "drive1",
            "parent-root",
            "document.txt",
            Bytes::from_static(b"my local changes"),
        )
        .await?;
    assert_eq!(upload_result.etag.as_deref(), Some("etag-v3-after-upload"));

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 11.5 — CACHE EVICTION (LRU by size)
// ============================================================================

#[tokio::test]
async fn test_cache_eviction_lru() -> cloudmount_core::Result<()> {
    let base = unique_temp_dir("eviction");
    let content_dir = base.join("content");
    let db_path = base.join("tracker.db");
    cleanup(&base);
    std::fs::create_dir_all(&content_dir)?;

    // Max 50 bytes — each file is 20 bytes, so 3rd insert triggers eviction of oldest
    let cache = DiskCache::new(content_dir, 50, &db_path);

    cache
        .put("d1", "oldest", b"AAAAAAAAAAAAAAAAAAAA", None)
        .await?; // 20 bytes
    sleep(Duration::from_millis(10)).await;
    cache
        .put("d1", "middle", b"BBBBBBBBBBBBBBBBBBBB", None)
        .await?; // 20 bytes → 40 total
    sleep(Duration::from_millis(10)).await;
    cache
        .put("d1", "newest", b"CCCCCCCCCCCCCCCCCCCC", None)
        .await?; // 20 bytes → 60 → evict oldest

    assert!(
        cache.get("d1", "oldest").await.is_none(),
        "oldest item should be evicted"
    );
    assert!(
        cache.get("d1", "middle").await.is_some(),
        "middle item should survive"
    );
    assert!(
        cache.get("d1", "newest").await.is_some(),
        "newest item should survive"
    );

    // Verify total size is within limit
    let total = cache.total_size();
    assert!(total <= 50, "total size {total} should be <= 50");

    // Insert another item — should evict "middle" now
    sleep(Duration::from_millis(10)).await;
    cache
        .put("d1", "extra", b"DDDDDDDDDDDDDDDDDDDD", None)
        .await?;

    assert!(
        cache.get("d1", "middle").await.is_none(),
        "middle should be evicted after 4th insert"
    );
    assert!(cache.get("d1", "newest").await.is_some());
    assert!(cache.get("d1", "extra").await.is_some());

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 11.6 — CRASH RECOVERY (pending writes survive restart)
// ============================================================================

#[tokio::test]
async fn test_crash_recovery_pending_writes() -> cloudmount_core::Result<()> {
    let base = unique_temp_dir("crash-recovery");
    cleanup(&base);
    std::fs::create_dir_all(&base)?;

    // Simulate "before crash": write and persist pending items then drop the buffer
    {
        let buffer = WriteBackBuffer::new(base.clone());
        buffer
            .write("drive1", "doc-a", b"unsaved content A")
            .await?;
        buffer.persist("drive1", "doc-a").await?;
        buffer
            .write("drive1", "doc-b", b"unsaved content B")
            .await?;
        buffer.persist("drive1", "doc-b").await?;
        buffer
            .write("drive2", "spreadsheet", b"unsaved spreadsheet")
            .await?;
        buffer.persist("drive2", "spreadsheet").await?;
    }
    // Buffer dropped — simulates process crash

    // Simulate "after restart": new buffer at same path should recover pending writes
    {
        let buffer = WriteBackBuffer::new(base.clone());
        let pending = buffer.list_pending().await?;

        assert_eq!(pending.len(), 3, "all 3 pending writes should survive");
        assert!(pending.contains(&("drive1".to_string(), "doc-a".to_string())));
        assert!(pending.contains(&("drive1".to_string(), "doc-b".to_string())));
        assert!(pending.contains(&("drive2".to_string(), "spreadsheet".to_string())));

        // Verify content is intact
        let content_a = buffer.read("drive1", "doc-a").await;
        assert_eq!(content_a.unwrap(), b"unsaved content A");

        let content_b = buffer.read("drive1", "doc-b").await;
        assert_eq!(content_b.unwrap(), b"unsaved content B");

        let content_ss = buffer.read("drive2", "spreadsheet").await;
        assert_eq!(content_ss.unwrap(), b"unsaved spreadsheet");
    }

    // Simulate recovery: upload and remove pending items
    {
        let buffer = WriteBackBuffer::new(base.clone());
        buffer.remove("drive1", "doc-a").await?;
        buffer.remove("drive1", "doc-b").await?;
        buffer.remove("drive2", "spreadsheet").await?;

        let remaining = buffer.list_pending().await?;
        assert!(
            remaining.is_empty(),
            "all pending writes should be cleared after recovery"
        );
    }

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 11.9 — PRE-CONFIGURED BUILD (PackagedDefaults merge) — removed in pivot-to-product
// ============================================================================

// (tests removed: PackagedDefaults system has been removed)

// Tests removed: PackagedDefaults system has been removed in pivot-to-product.
// 11.10 — UPDATE SCENARIO also removed (packaged mount override/dismiss logic gone).

// ============================================================================
// 11.7 — CROSS-PLATFORM SMOKE TEST: macOS (FUSE)
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires macOS with macFUSE installed"]
#[cfg(target_os = "macos")]
async fn test_smoke_macos_fuse_mount_list_read_write_unmount() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::MountHandle;
    use cloudmount_vfs::inode::InodeTable;

    let server = MockServer::start().await;
    let test_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let base = std::env::temp_dir().join(format!("cloudmount-smoke-macos-{test_id}"));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    let mountpoint = base.join("mnt");
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::create_dir_all(&mountpoint)?;

    let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
        Ok("test-token".to_string())
    }));
    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(300),
    )?);
    let inodes = Arc::new(InodeTable::new());
    inodes.set_root("root-id");

    let drive_id = "smoke-drive";

    Mock::given(method("GET"))
        .and(path(format!("/drives/{drive_id}/items/root-id/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                {
                    "id": "smoke-file-1",
                    "name": "readme.txt",
                    "size": 11,
                    "parentReference": { "driveId": drive_id, "id": "root-id" },
                    "file": { "mimeType": "text/plain" }
                }
            ]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{drive_id}/items/smoke-file-1/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(b"hello smoke".to_vec(), "text/plain"),
        )
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{drive_id}/items/root-id:/newfile.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "new-file-id",
            "name": "newfile.txt",
            "size": 9,
            "parentReference": { "driveId": drive_id, "id": "root-id" },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    let rt = tokio::runtime::Handle::current();
    let mount = MountHandle::mount(
        graph,
        cache,
        inodes,
        drive_id.to_string(),
        mountpoint.to_str().unwrap(),
        rt,
    )?;

    sleep(Duration::from_millis(300)).await;

    let entries: Vec<String> = std::fs::read_dir(&mountpoint)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        entries.contains(&"readme.txt".to_string()),
        "list: {entries:?}"
    );

    let content = std::fs::read_to_string(mountpoint.join("readme.txt"))?;
    assert_eq!(content, "hello smoke");

    std::fs::write(mountpoint.join("newfile.txt"), "new smoke")?;
    sleep(Duration::from_millis(200)).await;

    let _ = mount.unmount();
    sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&base);

    Ok(())
}

// ============================================================================
// 15.1 — INITIALIZATION SEQUENCE: config → auth → graph → cache → assembly
// ============================================================================

#[tokio::test]
async fn test_initialization_sequence() -> cloudmount_core::Result<()> {
    use cloudmount_auth::AuthManager;
    use cloudmount_vfs::inode::InodeTable;

    let base = unique_temp_dir("init-seq");
    cleanup(&base);
    std::fs::create_dir_all(&base)?;

    // 1. Load config (same sequence as run_desktop)
    let user_config = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&user_config);
    assert_eq!(effective.cache_max_size, "5GB");
    assert_eq!(effective.sync_interval_secs, 60);
    assert_eq!(effective.root_dir, "Cloud");

    // 2. Create AuthManager (CLIENT_ID is hardcoded in the app binary)
    let auth = Arc::new(AuthManager::new(
        "8ebe3ef7-f509-4146-8fef-c9b5d7c22252".to_string(),
        None,
        Arc::new(|_url: &str| Ok(())),
    ));

    // 3. Create GraphClient with auth token closure (same wiring as run_desktop)
    let auth_for_graph = auth.clone();
    let _graph_with_auth = Arc::new(GraphClient::with_base_url(
        "http://localhost:0".to_string(),
        move || {
            let auth = auth_for_graph.clone();
            async move { auth.access_token().await }
        },
    ));

    // Verify the auth-wired client correctly propagates auth errors
    let token_err = _graph_with_auth.get_my_drive().await;
    assert!(token_err.is_err(), "should fail without auth tokens");

    // Use a static-token client for the remaining integration checks
    let server = MockServer::start().await;
    let graph = Arc::new(make_client(&server.uri()));

    // 4. Create CacheManager
    let cache_dir = base.join("cache");
    let db_path = base.join("cloudmount.db");
    std::fs::create_dir_all(&cache_dir)?;
    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        5 * 1024 * 1024 * 1024,
        Some(60),
    )?);

    // 5. Create InodeTable and drive_ids
    let inodes = Arc::new(InodeTable::new());
    let drive_ids: Arc<std::sync::RwLock<Vec<String>>> =
        Arc::new(std::sync::RwLock::new(Vec::new()));

    assert!(inodes.get_item_id(1).is_none());
    assert!(drive_ids.read().unwrap().is_empty());

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "test-drive-001",
            "name": "My OneDrive",
            "driveType": "personal",
        })))
        .mount(&server)
        .await;

    let drive = graph.get_my_drive().await?;
    assert_eq!(drive.id, "test-drive-001");
    assert_eq!(drive.name, "My OneDrive");

    // Verify CacheManager tiers work together
    let item = test_drive_item("item-init-1", "init-test.txt", false);
    let ino = inodes.allocate("item-init-1");
    cache.memory.insert(ino, item.clone());
    cache
        .sqlite
        .upsert_item(ino, "test-drive-001", &item, None)?;
    cache
        .disk
        .put("test-drive-001", "item-init-1", b"init-content", None)
        .await?;

    assert_eq!(cache.memory.get(ino).unwrap().name, "init-test.txt");
    assert_eq!(
        cache
            .disk
            .get("test-drive-001", "item-init-1")
            .await
            .unwrap(),
        b"init-content"
    );

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 15.2 — TOKEN RESTORATION: verify try_restore logic
// ============================================================================

#[tokio::test]
async fn test_token_restoration_flow() -> cloudmount_core::Result<()> {
    use cloudmount_auth::AuthManager;
    use cloudmount_vfs::inode::InodeTable;

    // AuthManager::try_restore calls storage::load_tokens internally.
    // We can't easily mock the keyring/encrypted file storage, but we can
    // verify the "no tokens found" path (which is the common first-run case).
    let auth = Arc::new(AuthManager::new(
        "test-client-id".to_string(),
        Some("test-tenant".to_string()),
        Arc::new(|_url: &str| Ok(())),
    ));

    // try_restore returns false when no stored tokens exist
    let restored = auth.try_restore("nonexistent-account-id").await?;
    assert!(!restored, "should return false when no stored tokens exist");

    // Verify the auth state: access_token should fail (no tokens)
    let token_result = auth.access_token().await;
    assert!(
        token_result.is_err(),
        "access_token should fail without stored tokens"
    );

    // Now verify that the initialization logic correctly branches:
    // If restored=false and first_run=true → wizard should open
    // If restored=false and first_run=false → no mounts start
    // This is the logic in setup_after_launch (tested here without Tauri)

    let base = unique_temp_dir("token-restore");
    cleanup(&base);
    std::fs::create_dir_all(&base)?;

    let cache_dir = base.join("cache");
    let db_path = base.join("cloudmount.db");
    std::fs::create_dir_all(&cache_dir)?;
    let _cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(60),
    )?);
    let inodes = Arc::new(InodeTable::new());
    let drive_ids: Arc<std::sync::RwLock<Vec<String>>> =
        Arc::new(std::sync::RwLock::new(Vec::new()));

    assert!(drive_ids.read().unwrap().is_empty());
    assert!(inodes.get_item_id(1).is_none());

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 15.3 — SIGN-IN FLOW: mock OAuth, OneDrive discovery, mount config creation
// ============================================================================

#[tokio::test]
async fn test_sign_in_onedrive_discovery_and_mount_config() -> cloudmount_core::Result<()> {
    use cloudmount_core::config::derive_mount_point;

    let server = MockServer::start().await;

    // Mock /me/drive → OneDrive discovery
    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "user-drive-abc",
            "name": "User's OneDrive",
            "driveType": "personal",
        })))
        .mount(&server)
        .await;

    let graph = Arc::new(make_client(&server.uri()));

    // Simulate the sign_in command's OneDrive discovery
    let drive = graph.get_my_drive().await?;
    assert_eq!(drive.id, "user-drive-abc");
    assert_eq!(drive.name, "User's OneDrive");

    // Simulate mount config creation (same logic as sign_in command)
    let mut user_config = UserConfig::load("")?;
    assert!(user_config.accounts.is_empty());

    // Add account metadata
    user_config
        .accounts
        .push(cloudmount_core::config::AccountMetadata {
            id: drive.id.clone(),
            email: None,
            display_name: Some(drive.name.clone()),
            tenant_id: None,
        });

    // Auto-create OneDrive mount config with derived mount point
    let root_dir = "Cloud"; // default
    let mount_point = derive_mount_point(root_dir, "drive", None, None);
    assert!(
        mount_point.contains("Cloud"),
        "mount point should contain root_dir: {mount_point}"
    );
    assert!(
        mount_point.contains("OneDrive"),
        "mount point should contain OneDrive: {mount_point}"
    );

    let has_onedrive = user_config.mounts.iter().any(|m| m.mount_type == "drive");
    assert!(!has_onedrive);
    user_config.add_onedrive_mount(&drive.id, &mount_point, None)?;

    assert_eq!(user_config.mounts.len(), 1);
    let mount = &user_config.mounts[0];
    assert_eq!(mount.mount_type, "drive");
    assert_eq!(mount.drive_id, Some("user-drive-abc".to_string()));
    assert!(mount.enabled);
    assert_eq!(mount.name, "OneDrive");

    // Rebuild effective config
    let effective = EffectiveConfig::build(&user_config);
    assert_eq!(effective.mounts.len(), 1);
    assert_eq!(effective.accounts.len(), 1);
    assert_eq!(effective.accounts[0].id, "user-drive-abc");

    Ok(())
}

// ============================================================================
// 15.4 — SIGN-OUT: verify mounts stopped, tokens cleared, config updated
// ============================================================================

#[tokio::test]
async fn test_sign_out_clears_account_and_config() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::inode::InodeTable;

    // Set up state simulating an authenticated session
    let mut user_config = UserConfig::load("")?;
    user_config
        .accounts
        .push(cloudmount_core::config::AccountMetadata {
            id: "drive-to-remove".to_string(),
            email: Some("user@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            tenant_id: None,
        });
    user_config.add_onedrive_mount(
        "drive-to-remove",
        "/tmp/cloudmount-test-signout/OneDrive",
        None,
    )?;

    assert_eq!(user_config.accounts.len(), 1);
    assert_eq!(user_config.mounts.len(), 1);

    // Simulate drive tracking
    let drive_ids: Arc<std::sync::RwLock<Vec<String>>> =
        Arc::new(std::sync::RwLock::new(vec!["drive-to-remove".to_string()]));
    let inodes = Arc::new(InodeTable::new());
    inodes.set_root("root-item");
    let _ino = inodes.allocate("some-file");

    // Simulate the sign_out command logic:
    // 1. Stop all mounts (clear drive_ids)
    drive_ids.write().unwrap().clear();

    // 2. Clear account metadata only — mounts are preserved for re-use on next sign-in
    user_config.accounts.clear();

    // 3. Save config
    let base = unique_temp_dir("signout");
    cleanup(&base);
    std::fs::create_dir_all(&base)?;
    let config_path = base.join("config.toml");
    user_config.save_to_file(&config_path)?;

    // Reload and verify
    let reloaded = UserConfig::load_from_file(&config_path)?;
    assert!(
        reloaded.accounts.is_empty(),
        "accounts should be cleared after sign-out"
    );
    assert_eq!(
        reloaded.mounts.len(),
        1,
        "mounts should be preserved after sign-out"
    );

    // drive_ids should be empty (mounts stopped)
    assert!(drive_ids.read().unwrap().is_empty());

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 15.9 — ACCOUNT-SCOPED MOUNTS: only mounts matching account_id are active
// ============================================================================

#[tokio::test]
async fn test_account_scoped_mounts_filtered() -> cloudmount_core::Result<()> {
    let mut user_config = UserConfig::load("")?;

    // Add two mounts for different accounts
    user_config.add_onedrive_mount(
        "drive-account-a",
        "/mnt/a/OneDrive",
        Some("account-a".to_string()),
    )?;
    user_config.add_onedrive_mount(
        "drive-account-b",
        "/mnt/b/OneDrive",
        Some("account-b".to_string()),
    )?;

    assert_eq!(user_config.mounts.len(), 2);

    // Simulate active account = "account-a" filtering
    let active_account = "account-a";
    let filtered: Vec<_> = user_config
        .mounts
        .iter()
        .filter(|m| m.account_id.as_deref() == Some(active_account))
        .collect();

    assert_eq!(filtered.len(), 1, "only account-a mount should be active");
    assert_eq!(filtered[0].drive_id, Some("drive-account-a".to_string()));

    // When no account is signed in, no mounts should be active
    let no_account_filtered: Vec<_> = user_config
        .mounts
        .iter()
        .filter(|m| m.account_id.as_deref() == Some("nonexistent"))
        .collect();
    assert!(no_account_filtered.is_empty());

    Ok(())
}

// ============================================================================
// 15.5 — CRASH RECOVERY: pending writes detected and re-uploaded
// ============================================================================

#[tokio::test]
async fn test_crash_recovery_reupload_pending_writes() -> cloudmount_core::Result<()> {
    let server = MockServer::start().await;
    let base = unique_temp_dir("crash-reupload");
    let cache_dir = base.join("cache");
    let db_path = base.join("cloudmount.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    let graph = Arc::new(make_client(&server.uri()));
    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(60),
    )?);

    // Simulate crashed writes: write and persist pending items
    cache
        .writeback
        .write("drive-crash", "doc-1", b"unsaved document content")
        .await?;
    cache.writeback.persist("drive-crash", "doc-1").await?;
    cache
        .writeback
        .write("drive-crash", "doc-2", b"another unsaved file")
        .await?;
    cache.writeback.persist("drive-crash", "doc-2").await?;

    // Verify pending writes exist on disk
    let pending = cache.writeback.list_pending().await?;
    assert_eq!(pending.len(), 2, "should have 2 pending writes");

    // Mock upload endpoints for crash recovery
    Mock::given(method("PUT"))
        .and(path("/drives/drive-crash/items/:/doc-1:/content"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "doc-1",
            "name": "doc-1",
            "size": 24,
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/drives/drive-crash/items/:/doc-2:/content"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "doc-2",
            "name": "doc-2",
            "size": 20,
        })))
        .mount(&server)
        .await;

    // Simulate crash recovery logic (same as run_crash_recovery):
    // For each pending write, read content, upload, remove on success
    for (drive_id, item_id) in &pending {
        let content = cache.writeback.read(drive_id, item_id).await;
        assert!(content.is_some(), "should be able to read pending write");

        let content = content.unwrap();
        let upload_result = graph
            .upload(
                drive_id,
                "",
                Some(item_id.as_str()),
                item_id,
                Bytes::from(content),
            )
            .await;
        assert!(
            upload_result.is_ok(),
            "upload should succeed: {:?}",
            upload_result.err()
        );

        cache.writeback.remove(drive_id, item_id).await?;
    }

    // Verify all pending writes cleared
    let remaining = cache.writeback.list_pending().await?;
    assert!(
        remaining.is_empty(),
        "all pending writes should be cleared after recovery"
    );

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 15.6 — GRACEFUL SHUTDOWN: verify sync cancel, mounts stop, cleanup
// ============================================================================

#[tokio::test]
async fn test_graceful_shutdown_stops_sync_and_mounts() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::inode::InodeTable;
    use tokio_util::sync::CancellationToken;

    let base = unique_temp_dir("shutdown");
    let cache_dir = base.join("cache");
    let db_path = base.join("cloudmount.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(60),
    )?);
    let _inodes = Arc::new(InodeTable::new());
    let drive_ids: Arc<std::sync::RwLock<Vec<String>>> = Arc::new(std::sync::RwLock::new(vec![
        "drive-1".to_string(),
        "drive-2".to_string(),
    ]));

    // Simulate active sync via CancellationToken
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let sync_active = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let sync_active_clone = sync_active.clone();

    let sync_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    sync_active_clone.store(false, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
            }
        }
    });

    // Verify sync is running
    assert!(sync_active.load(std::sync::atomic::Ordering::Relaxed));

    // Perform graceful shutdown logic:
    // 1. Cancel sync timer
    cancel.cancel();
    // Wait for the sync task to acknowledge cancellation
    let _ = tokio::time::timeout(Duration::from_secs(2), sync_handle).await;
    assert!(
        !sync_active.load(std::sync::atomic::Ordering::Relaxed),
        "sync should be stopped after cancel"
    );

    // 2. Stop all mounts (clear tracking)
    drive_ids.write().unwrap().clear();
    assert!(
        drive_ids.read().unwrap().is_empty(),
        "all drives should be cleared"
    );

    // 3. Verify cache can still flush (pending writes survive shutdown)
    cache
        .writeback
        .write("drive-1", "last-write", b"flushed before exit")
        .await?;
    let content = cache.writeback.read("drive-1", "last-write").await;
    assert_eq!(
        content.unwrap(),
        b"flushed before exit",
        "writes before shutdown should persist"
    );

    cleanup(&base);
    Ok(())
}

// ============================================================================
// 15.7 — AUTH DEGRADATION: detect auth errors, set flag, continue syncing
// ============================================================================

#[tokio::test]
async fn test_auth_degradation_detection_in_delta_sync() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::inode::InodeTable;
    use std::sync::atomic::{AtomicBool, Ordering};

    let server = MockServer::start().await;
    let base = unique_temp_dir("auth-degrade");
    let cache_dir = base.join("cache");
    let db_path = base.join("cloudmount.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    // Create a GraphClient whose token function returns an auth error
    // simulating an expired/revoked refresh token
    let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
        Err(cloudmount_core::Error::Auth(
            "re-authentication required".to_string(),
        ))
    }));

    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(60),
    )?);
    let inodes = Arc::new(InodeTable::new());
    let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        Arc::new(move |item_id: &str| inodes.allocate(item_id));

    let auth_degraded = AtomicBool::new(false);

    // Run delta sync — should fail with auth error
    let result =
        cloudmount_cache::sync::run_delta_sync(&graph, &cache, "test-drive", &inode_allocator)
            .await;

    // Simulate the auth degradation detection logic from start_delta_sync:
    match result {
        Err(cloudmount_core::Error::Auth(ref msg))
            if msg.contains("re-authentication required") =>
        {
            auth_degraded.store(true, Ordering::Relaxed);
        }
        Err(e) => {
            panic!("expected Auth error, got: {e:?}");
        }
        Ok(()) => {
            panic!("expected Auth error, but delta sync succeeded");
        }
    }

    assert!(
        auth_degraded.load(Ordering::Relaxed),
        "auth_degraded flag should be set after auth error"
    );

    // Simulate re-sign-in clearing degradation
    auth_degraded.store(false, Ordering::Relaxed);
    assert!(
        !auth_degraded.load(Ordering::Relaxed),
        "auth_degraded should be cleared after re-sign-in"
    );

    // Now verify that a working GraphClient produces successful delta sync
    let graph_ok = Arc::new(make_client(&server.uri()));

    Mock::given(method("GET"))
        .and(path("/drives/test-drive/root/delta"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [],
            "@odata.deltaLink": "https://example.com/delta?token=new",
        })))
        .mount(&server)
        .await;

    let inodes2 = Arc::new(InodeTable::new());
    let inode_allocator2: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        Arc::new(move |item_id: &str| inodes2.allocate(item_id));

    let result2 =
        cloudmount_cache::sync::run_delta_sync(&graph_ok, &cache, "test-drive", &inode_allocator2)
            .await;
    assert!(
        result2.is_ok(),
        "delta sync should succeed with valid auth: {:?}",
        result2.err()
    );

    cleanup(&base);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Windows with Cloud Files API"]
#[cfg(target_os = "windows")]
async fn test_smoke_windows_cfapi_mount_list_read_write_unmount() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::CfMountHandle;
    use cloudmount_vfs::inode::InodeTable;

    let server = MockServer::start().await;
    let test_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let base = std::env::temp_dir().join(format!("cloudmount-smoke-win-{test_id}"));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    let sync_root = base.join("sync");
    std::fs::create_dir_all(&cache_dir)?;

    let graph = Arc::new(GraphClient::with_base_url(server.uri(), || async {
        Ok("test-token".to_string())
    }));
    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(300),
    )?);
    let inodes = Arc::new(InodeTable::new());
    inodes.set_root("root-id");

    let drive_id = "smoke-drive";

    Mock::given(method("GET"))
        .and(path(format!("/drives/{drive_id}/items/root-id/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                {
                    "id": "smoke-file-1",
                    "name": "readme.txt",
                    "size": 11,
                    "parentReference": { "driveId": drive_id, "id": "root-id" },
                    "file": { "mimeType": "text/plain" }
                }
            ]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{drive_id}/items/smoke-file-1/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(b"hello smoke".to_vec(), "text/plain"),
        )
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path(format!(
            "/drives/{drive_id}/items/root-id:/newfile.txt:/content"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "new-file-id",
            "name": "newfile.txt",
            "size": 9,
            "parentReference": { "driveId": drive_id, "id": "root-id" },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/drives/{drive_id}/items/smoke-file-1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "smoke-file-1",
            "name": "readme.txt",
            "size": 11,
            "parentReference": { "driveId": drive_id, "id": "root-id" },
            "file": { "mimeType": "text/plain" }
        })))
        .mount(&server)
        .await;

    let rt = tokio::runtime::Handle::current();
    let mount = CfMountHandle::mount(
        graph,
        cache,
        inodes,
        drive_id.to_string(),
        &sync_root,
        rt,
        drive_id.to_string(),
    )?;

    sleep(Duration::from_millis(500)).await;

    let entries: Vec<String> = std::fs::read_dir(&sync_root)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        entries.contains(&"readme.txt".to_string()),
        "list: {entries:?}"
    );

    let content = std::fs::read_to_string(sync_root.join("readme.txt"))?;
    assert_eq!(content, "hello smoke");

    std::fs::write(sync_root.join("newfile.txt"), "new smoke")?;
    sleep(Duration::from_secs(2)).await;

    let _ = mount.unmount();
    sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&base);

    Ok(())
}

// ============================================================================
// SURGICAL INVALIDATION: parent cache preserved after create
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_surgical_invalidation_create_preserves_parent_cache() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::core_ops::CoreOps;
    use cloudmount_vfs::inode::InodeTable;

    let server = MockServer::start().await;
    let base = unique_temp_dir("surgical");
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    cleanup(&base);
    std::fs::create_dir_all(&cache_dir)?;

    let graph = Arc::new(make_client(&server.uri()));
    let cache = Arc::new(CacheManager::new(
        cache_dir,
        db_path,
        100_000_000,
        Some(300),
    )?);
    let inodes = Arc::new(InodeTable::new());
    inodes.set_root("root-id");
    let drive_id = "test-drive";

    // Seed root directory item in cache so insert_with_children can find the parent
    let root_item = test_drive_item("root-id", "root", true);
    cache.memory.insert(1, root_item);

    // Mock list_children for root — should only be called once
    let list_children_mock = Mock::given(method("GET"))
        .and(path(format!("/drives/{drive_id}/items/root-id/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                {
                    "id": "existing-file-1",
                    "name": "existing.txt",
                    "size": 100,
                    "parentReference": { "driveId": drive_id, "id": "root-id" },
                    "file": { "mimeType": "text/plain" }
                }
            ]
        })))
        .expect(1) // Exactly one call
        .mount_as_scoped(&server)
        .await;

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(
        graph.clone(),
        cache.clone(),
        inodes.clone(),
        drive_id.to_string(),
        rt,
    );

    // CoreOps uses block_on internally, so run on a blocking thread
    tokio::task::spawn_blocking(move || {
        // First call populates the parent's children cache from Graph API
        let children = ops.list_children(1);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].1.name, "existing.txt");

        // Create a new file — should surgically add to parent cache, NOT invalidate it
        let create_result = ops.create_file(1, "newfile.txt");
        assert!(create_result.is_ok());

        // List children again — should serve from memory cache, no new Graph API call
        let children_after = ops.list_children(1);
        assert_eq!(
            children_after.len(),
            2,
            "should have both existing and new file"
        );
        let names: Vec<&str> = children_after
            .iter()
            .map(|(_, item)| item.name.as_str())
            .collect();
        assert!(names.contains(&"existing.txt"));
        assert!(names.contains(&"newfile.txt"));
    })
    .await
    .expect("blocking task panicked");

    // The scoped mock with expect(1) will panic on drop if called more than once
    drop(list_children_mock);

    cleanup(&base);
    Ok(())
}
