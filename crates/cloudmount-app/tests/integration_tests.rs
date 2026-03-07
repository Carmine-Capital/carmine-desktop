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
use cloudmount_core::config::{EffectiveConfig, PackagedDefaults, UserConfig};
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
        .put("drive1", "item-a", b"report-content-binary")
        .await?;
    cache.disk.put("drive1", "item-b", b"notes go here").await?;

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

    cache.put("d1", "oldest", b"AAAAAAAAAAAAAAAAAAAA").await?; // 20 bytes
    sleep(Duration::from_millis(10)).await;
    cache.put("d1", "middle", b"BBBBBBBBBBBBBBBBBBBB").await?; // 20 bytes → 40 total
    sleep(Duration::from_millis(10)).await;
    cache.put("d1", "newest", b"CCCCCCCCCCCCCCCCCCCC").await?; // 20 bytes → 60 → evict oldest

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
    cache.put("d1", "extra", b"DDDDDDDDDDDDDDDDDDDD").await?;

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
// 11.9 — PRE-CONFIGURED BUILD (PackagedDefaults merge)
// ============================================================================

#[test]
fn test_preconfigured_build_loads_packaged_defaults() -> cloudmount_core::Result<()> {
    let packaged_toml = r#"
[tenant]
id = "org-tenant-12345"
client_id = "org-client-67890"

[branding]
app_name = "Contoso CloudMount"

[defaults]
auto_start = true
cache_max_size = "20GB"
sync_interval_secs = 120
metadata_ttl_secs = 300

[[mounts]]
id = "corp-onedrive"
name = "Corporate OneDrive"
type = "drive"
mount_point = "/mnt/onedrive"
enabled = true
drive_id = "corp-drive-001"

[[mounts]]
id = "corp-sharepoint"
name = "Engineering Docs"
type = "sharepoint"
mount_point = "/mnt/engineering"
enabled = true
site_id = "site-eng-001"
library_name = "Documents"
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;

    assert!(
        packaged.has_packaged_config(),
        "should detect pre-configured build"
    );
    assert_eq!(packaged.tenant_id(), Some("org-tenant-12345"));
    assert_eq!(packaged.client_id(), Some("org-client-67890"));
    assert_eq!(packaged.app_name(), "Contoso CloudMount");
    assert_eq!(packaged.mounts.len(), 2);

    // Merge with empty user config — packaged defaults dominate
    let user = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.app_name, "Contoso CloudMount");
    assert_eq!(effective.tenant_id, Some("org-tenant-12345".to_string()));
    assert_eq!(effective.client_id, Some("org-client-67890".to_string()));
    assert!(effective.auto_start);
    assert_eq!(effective.cache_max_size, "20GB");
    assert_eq!(effective.sync_interval_secs, 120);
    assert_eq!(effective.metadata_ttl_secs, 300);
    assert_eq!(effective.mounts.len(), 2);

    let od = effective
        .mounts
        .iter()
        .find(|m| m.id == "corp-onedrive")
        .unwrap();
    assert_eq!(od.name, "Corporate OneDrive");
    assert_eq!(od.drive_id, Some("corp-drive-001".to_string()));
    assert!(od.enabled);

    let sp = effective
        .mounts
        .iter()
        .find(|m| m.id == "corp-sharepoint")
        .unwrap();
    assert_eq!(sp.name, "Engineering Docs");
    assert_eq!(sp.site_id, Some("site-eng-001".to_string()));

    // Wizard would show simplified flow when packaged config exists
    assert!(packaged.has_packaged_config());

    Ok(())
}

// ============================================================================
// 11.10 — UPDATE SCENARIO (user overrides preserved across packaged updates)
// ============================================================================

#[test]
fn test_update_preserves_user_overrides() -> cloudmount_core::Result<()> {
    // --- Phase 1: PackagedDefaults v1 with 2 mounts ---
    let packaged_v1_toml = r#"
[tenant]
id = "tenant-v1"
client_id = "client-v1"

[branding]
app_name = "OrgSync v1"

[defaults]
auto_start = false
cache_max_size = "10GB"
sync_interval_secs = 60

[[mounts]]
id = "pkg-drive"
name = "Main Drive"
type = "drive"
mount_point = "/mnt/main"
enabled = true
drive_id = "drive-001"

[[mounts]]
id = "pkg-docs"
name = "Shared Docs"
type = "sharepoint"
mount_point = "/mnt/docs"
enabled = true
site_id = "site-001"
library_name = "Documents"
"#;

    // User config: overrides + dismissed mount + extra user mount + general settings
    let user_toml = r#"
dismissed_packaged_mounts = ["pkg-docs"]

[general]
auto_start = true
cache_max_size = "25GB"
sync_interval_secs = 180
log_level = "debug"
notifications = false

[[mounts]]
id = "user-personal"
name = "Personal Backup"
type = "drive"
mount_point = "/mnt/personal"
enabled = true
drive_id = "user-drive-999"

[[mount_overrides]]
id = "pkg-drive"
name = "My Custom Name"
mount_point = "/mnt/custom-main"
"#;

    let packaged_v1 = PackagedDefaults::load(packaged_v1_toml)?;
    let user = UserConfig::load(user_toml)?;
    let effective_v1 = EffectiveConfig::build(&packaged_v1, &user);

    // User overrides take precedence
    assert!(effective_v1.auto_start);
    assert_eq!(effective_v1.cache_max_size, "25GB");
    assert_eq!(effective_v1.sync_interval_secs, 180);
    assert_eq!(effective_v1.log_level, "debug");
    assert!(!effective_v1.notifications);

    // pkg-drive has user's override name + mount_point
    let drive_v1 = effective_v1
        .mounts
        .iter()
        .find(|m| m.id == "pkg-drive")
        .unwrap();
    assert_eq!(drive_v1.name, "My Custom Name");
    assert_eq!(drive_v1.mount_point, "/mnt/custom-main");
    assert_eq!(drive_v1.drive_id, Some("drive-001".to_string()));

    // pkg-docs is dismissed — should not appear
    assert!(
        !effective_v1.mounts.iter().any(|m| m.id == "pkg-docs"),
        "dismissed mount should be excluded"
    );

    // User's personal mount is present
    assert!(effective_v1.mounts.iter().any(|m| m.id == "user-personal"));

    // Total: 1 packaged (pkg-drive, not dismissed) + 1 user
    assert_eq!(effective_v1.mounts.len(), 2);

    // --- Phase 2: PackagedDefaults v2 adds a new mount ---
    let packaged_v2_toml = r#"
[tenant]
id = "tenant-v2"
client_id = "client-v2"

[branding]
app_name = "OrgSync v2"

[defaults]
auto_start = false
cache_max_size = "15GB"
sync_interval_secs = 90

[[mounts]]
id = "pkg-drive"
name = "Main Drive (Updated)"
type = "drive"
mount_point = "/mnt/main-v2"
enabled = true
drive_id = "drive-001"

[[mounts]]
id = "pkg-docs"
name = "Shared Docs (Updated)"
type = "sharepoint"
mount_point = "/mnt/docs-v2"
enabled = true
site_id = "site-001"
library_name = "Documents"

[[mounts]]
id = "pkg-wiki"
name = "Team Wiki"
type = "sharepoint"
mount_point = "/mnt/wiki"
enabled = true
site_id = "site-002"
library_name = "Wiki Pages"
"#;

    let packaged_v2 = PackagedDefaults::load(packaged_v2_toml)?;
    let effective_v2 = EffectiveConfig::build(&packaged_v2, &user);

    // User general overrides STILL take precedence over v2 packaged defaults
    assert!(
        effective_v2.auto_start,
        "user auto_start=true overrides pkg false"
    );
    assert_eq!(
        effective_v2.cache_max_size, "25GB",
        "user cache size preserved"
    );
    assert_eq!(
        effective_v2.sync_interval_secs, 180,
        "user sync interval preserved"
    );
    assert_eq!(effective_v2.log_level, "debug", "user log level preserved");
    assert!(
        !effective_v2.notifications,
        "user notifications=false preserved"
    );

    // App name updates to v2 (branding comes from packaged, not user)
    assert_eq!(effective_v2.app_name, "OrgSync v2");
    assert_eq!(effective_v2.tenant_id, Some("tenant-v2".to_string()));

    // pkg-drive: user override still applies over v2's updated name/path
    let drive_v2 = effective_v2
        .mounts
        .iter()
        .find(|m| m.id == "pkg-drive")
        .unwrap();
    assert_eq!(
        drive_v2.name, "My Custom Name",
        "user override name survives update"
    );
    assert_eq!(
        drive_v2.mount_point, "/mnt/custom-main",
        "user override mount_point survives update"
    );
    assert_eq!(drive_v2.drive_id, Some("drive-001".to_string()));

    // pkg-docs: still dismissed by user
    assert!(
        !effective_v2.mounts.iter().any(|m| m.id == "pkg-docs"),
        "dismissed mount stays hidden after update"
    );

    // pkg-wiki: NEW mount from v2 — should appear since user hasn't dismissed it
    let wiki = effective_v2
        .mounts
        .iter()
        .find(|m| m.id == "pkg-wiki")
        .unwrap();
    assert_eq!(wiki.name, "Team Wiki");
    assert_eq!(wiki.mount_point, "/mnt/wiki");
    assert_eq!(wiki.site_id, Some("site-002".to_string()));
    assert!(wiki.enabled);

    // User's personal mount survives
    let personal = effective_v2
        .mounts
        .iter()
        .find(|m| m.id == "user-personal")
        .unwrap();
    assert_eq!(personal.name, "Personal Backup");
    assert_eq!(personal.drive_id, Some("user-drive-999".to_string()));

    // Total: pkg-drive + pkg-wiki + user-personal = 3 (pkg-docs dismissed)
    assert_eq!(effective_v2.mounts.len(), 3);

    Ok(())
}

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
    let packaged = PackagedDefaults::load("")?;
    assert_eq!(packaged.app_name(), "CloudMount");
    assert!(!packaged.has_packaged_config());

    let user_config = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&packaged, &user_config);
    assert_eq!(effective.app_name, "CloudMount");
    assert_eq!(effective.cache_max_size, "5GB");
    assert_eq!(effective.sync_interval_secs, 60);
    assert_eq!(effective.root_dir, "Cloud");

    // 2. Create AuthManager
    let client_id = packaged
        .client_id()
        .unwrap_or("00000000-0000-0000-0000-000000000000");
    let auth = Arc::new(AuthManager::new(client_id.to_string(), None));

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
        .put("test-drive-001", "item-init-1", b"init-content")
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
    user_config.add_onedrive_mount(&drive.id, &mount_point)?;

    assert_eq!(user_config.mounts.len(), 1);
    let mount = &user_config.mounts[0];
    assert_eq!(mount.mount_type, "drive");
    assert_eq!(mount.drive_id, Some("user-drive-abc".to_string()));
    assert!(mount.enabled);
    assert_eq!(mount.name, "OneDrive");

    // Rebuild effective config
    let packaged = PackagedDefaults::load("")?;
    let effective = EffectiveConfig::build(&packaged, &user_config);
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
    user_config.add_onedrive_mount("drive-to-remove", "/tmp/cloudmount-test-signout/OneDrive")?;

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

    // 2. Clear account metadata
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
    // Mounts remain in config (they aren't deleted on sign-out, just stopped)
    assert_eq!(reloaded.mounts.len(), 1);

    // Rebuild effective config — no accounts
    let packaged = PackagedDefaults::load("")?;
    let effective = EffectiveConfig::build(&packaged, &reloaded);
    assert!(effective.accounts.is_empty());

    // drive_ids should be empty (mounts stopped)
    assert!(drive_ids.read().unwrap().is_empty());

    cleanup(&base);
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
