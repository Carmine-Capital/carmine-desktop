use std::collections::HashSet;

use carminedesktop_cache::disk::DiskCache;
use carminedesktop_cache::memory::MemoryCache;
use carminedesktop_cache::pin_store::PinStore;
use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::DriveItem;

/// Helper to create a test DriveItem.
fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem {
    use carminedesktop_core::types::FolderFacet;
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
        publication: None,
        download_url: None,
        web_url: None,
    }
}

/// Create a unique temp directory for a test, cleaned before use.
fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("carmine_cache_stats_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ============================================================================
// DiskCache accessor tests
// ============================================================================

#[test]
fn test_disk_cache_max_size_bytes_returns_configured_value() {
    let dir = test_dir("dc_max_size");
    let db_path = dir.join("cache.db");
    let dc = DiskCache::new(dir.join("content"), 5_368_709_120, &db_path).unwrap();
    assert_eq!(dc.max_size_bytes(), 5_368_709_120);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_disk_cache_entry_count_empty_then_populated() {
    let dir = test_dir("dc_entry_count");
    let db_path = dir.join("cache.db");
    let dc = DiskCache::new(dir.join("content"), 1_000_000, &db_path).unwrap();

    assert_eq!(dc.entry_count(), 0);

    dc.put("drive-1", "item-a", b"hello", None).await.unwrap();
    assert_eq!(dc.entry_count(), 1);

    dc.put("drive-1", "item-b", b"world", None).await.unwrap();
    assert_eq!(dc.entry_count(), 2);

    dc.remove("drive-1", "item-a").await.unwrap();
    assert_eq!(dc.entry_count(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================================
// MemoryCache accessor tests
// ============================================================================

#[test]
fn test_memory_cache_len_empty_then_populated() {
    let mc = MemoryCache::new(Some(60));
    assert_eq!(mc.len(), 0);
    assert!(mc.is_empty());

    mc.insert(1, test_drive_item("item-1", "a.txt", false));
    assert_eq!(mc.len(), 1);
    assert!(!mc.is_empty());

    mc.insert(2, test_drive_item("item-2", "b.txt", false));
    assert_eq!(mc.len(), 2);

    mc.invalidate(1);
    assert_eq!(mc.len(), 1);

    mc.clear();
    assert_eq!(mc.len(), 0);
    assert!(mc.is_empty());
}

// ============================================================================
// CacheManager::stats() tests
// ============================================================================

#[tokio::test]
async fn test_cache_manager_stats_returns_correct_values() {
    let dir = test_dir("cm_stats");
    let db_path = dir.join("cache.db");
    let cm = CacheManager::new(dir.clone(), db_path, 10_000_000, Some(60), "drive-x".to_string())
        .unwrap();

    // Insert items into memory cache
    cm.memory.insert(1, test_drive_item("item-1", "a.txt", false));
    cm.memory.insert(2, test_drive_item("item-2", "b.txt", false));

    // Insert content into disk cache
    cm.disk.put("drive-x", "item-1", b"hello", None).await.unwrap();

    // Mark an inode dirty
    cm.dirty_inodes.insert(100);
    cm.dirty_inodes.insert(200);
    cm.dirty_inodes.insert(300);

    let stats = cm.stats();

    assert_eq!(stats.memory_entry_count, 2);
    assert_eq!(stats.disk_used_bytes, 5); // "hello" is 5 bytes
    assert_eq!(stats.disk_max_bytes, 10_000_000);
    assert_eq!(stats.dirty_inode_count, 3);

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================================
// PinStore::health() tests
// ============================================================================

#[tokio::test]
async fn test_pin_store_health_all_cached_is_downloaded() {
    let dir = test_dir("ps_health_downloaded");
    let db_path = dir.join("cache.db");

    // Create SqliteStore first (creates tables including items and pinned_folders)
    let _sqlite = carminedesktop_cache::sqlite::SqliteStore::open(&db_path).unwrap();

    // Create DiskCache (creates cache_entries table)
    let dc = DiskCache::new(dir.join("content"), 10_000_000, &db_path).unwrap();

    // Open PinStore on the same db
    let ps = PinStore::open(&db_path).unwrap();

    // Pin a folder with a long TTL
    ps.pin("drive-1", "folder-root", 86400).unwrap();

    // Insert the folder item into items table
    _sqlite
        .upsert_item(
            100,
            "drive-1",
            &test_drive_item("folder-root", "Reports", true),
            None,
        )
        .unwrap();

    // Insert 3 file items as children of the pinned folder
    for (inode, id, name) in [(101, "file-1", "a.txt"), (102, "file-2", "b.txt"), (103, "file-3", "c.txt")] {
        _sqlite
            .upsert_item(
                inode,
                "drive-1",
                &test_drive_item(id, name, false),
                Some(100),
            )
            .unwrap();
    }

    // Cache all 3 files in disk cache
    dc.put("drive-1", "file-1", b"aaa", None).await.unwrap();
    dc.put("drive-1", "file-2", b"bbb", None).await.unwrap();
    dc.put("drive-1", "file-3", b"ccc", None).await.unwrap();

    let stale_pins = HashSet::new();
    let health = ps.health(&stale_pins).unwrap();

    assert_eq!(health.len(), 1);
    let (pin, total, cached) = &health[0];
    assert_eq!(pin.drive_id, "drive-1");
    assert_eq!(pin.item_id, "folder-root");
    assert_eq!(*total, 3);
    assert_eq!(*cached, 3);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_pin_store_health_partial_when_some_files_missing() {
    let dir = test_dir("ps_health_partial");
    let db_path = dir.join("cache.db");

    let _sqlite = carminedesktop_cache::sqlite::SqliteStore::open(&db_path).unwrap();
    let dc = DiskCache::new(dir.join("content"), 10_000_000, &db_path).unwrap();
    let ps = PinStore::open(&db_path).unwrap();

    ps.pin("drive-1", "folder-root", 86400).unwrap();

    _sqlite
        .upsert_item(
            100,
            "drive-1",
            &test_drive_item("folder-root", "Reports", true),
            None,
        )
        .unwrap();

    // 3 files, but only 1 cached
    for (inode, id, name) in [(101, "file-1", "a.txt"), (102, "file-2", "b.txt"), (103, "file-3", "c.txt")] {
        _sqlite
            .upsert_item(
                inode,
                "drive-1",
                &test_drive_item(id, name, false),
                Some(100),
            )
            .unwrap();
    }

    dc.put("drive-1", "file-1", b"aaa", None).await.unwrap();
    // file-2 and file-3 are NOT cached

    let stale_pins = HashSet::new();
    let health = ps.health(&stale_pins).unwrap();

    assert_eq!(health.len(), 1);
    let (_, total, cached) = &health[0];
    assert_eq!(*total, 3);
    assert_eq!(*cached, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_pin_store_health_counts_match_actual_data() {
    let dir = test_dir("ps_health_counts");
    let db_path = dir.join("cache.db");

    let _sqlite = carminedesktop_cache::sqlite::SqliteStore::open(&db_path).unwrap();
    let dc = DiskCache::new(dir.join("content"), 10_000_000, &db_path).unwrap();
    let ps = PinStore::open(&db_path).unwrap();

    // Pin a folder
    ps.pin("drive-1", "folder-top", 86400).unwrap();

    // Create folder hierarchy: folder-top -> subfolder -> files
    _sqlite
        .upsert_item(
            100,
            "drive-1",
            &test_drive_item("folder-top", "Top", true),
            None,
        )
        .unwrap();
    _sqlite
        .upsert_item(
            200,
            "drive-1",
            &test_drive_item("subfolder", "Sub", true),
            Some(100),
        )
        .unwrap();

    // 2 files directly under folder-top
    _sqlite
        .upsert_item(
            101,
            "drive-1",
            &test_drive_item("file-a", "a.txt", false),
            Some(100),
        )
        .unwrap();
    _sqlite
        .upsert_item(
            102,
            "drive-1",
            &test_drive_item("file-b", "b.txt", false),
            Some(100),
        )
        .unwrap();

    // 1 file under subfolder
    _sqlite
        .upsert_item(
            201,
            "drive-1",
            &test_drive_item("file-c", "c.txt", false),
            Some(200),
        )
        .unwrap();

    // Cache 2 of 3 files
    dc.put("drive-1", "file-a", b"aaa", None).await.unwrap();
    dc.put("drive-1", "file-c", b"ccc", None).await.unwrap();

    let stale_pins = HashSet::new();
    let health = ps.health(&stale_pins).unwrap();

    assert_eq!(health.len(), 1);
    let (_, total, cached) = &health[0];
    assert_eq!(*total, 3); // 3 files total across the tree
    assert_eq!(*cached, 2); // 2 are in disk cache

    let _ = std::fs::remove_dir_all(&dir);
}
