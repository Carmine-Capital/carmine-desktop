use filesync_cache::{
    disk::DiskCache, memory::MemoryCache, sqlite::SqliteStore, writeback::WriteBackBuffer,
};
use filesync_core::types::{DriveItem, FolderFacet};
use std::time::Duration;
use tokio::time::sleep;

/// Helper function to create a test DriveItem
fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 1024,
        last_modified: None,
        created: None,
        etag: Some(format!("etag-{}", id)),
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

// ============================================================================
// MEMORY CACHE TESTS
// ============================================================================

#[test]
fn test_memory_cache_insert_get_roundtrip() {
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item.clone());
    let retrieved = cache.get(1);

    assert!(retrieved.is_some());
    let retrieved_item = retrieved.unwrap();
    assert_eq!(retrieved_item.id, "item1");
    assert_eq!(retrieved_item.name, "test.txt");
}

#[test]
fn test_memory_cache_ttl_expiry() {
    let cache = MemoryCache::new(Some(1));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item);
    assert!(cache.get(1).is_some());

    std::thread::sleep(Duration::from_secs(2));
    assert!(cache.get(1).is_none());
}

#[test]
fn test_memory_cache_invalidate() {
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item);
    assert!(cache.get(1).is_some());

    cache.invalidate(1);
    assert!(cache.get(1).is_none());
}

#[test]
fn test_memory_cache_clear() {
    let cache = MemoryCache::new(Some(60));
    let item1 = test_drive_item("item1", "test1.txt", false);
    let item2 = test_drive_item("item2", "test2.txt", false);

    cache.insert(1, item1);
    cache.insert(2, item2);
    assert!(cache.get(1).is_some());
    assert!(cache.get(2).is_some());

    cache.clear();
    assert!(cache.get(1).is_none());
    assert!(cache.get(2).is_none());
}

#[test]
fn test_memory_cache_insert_with_children() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children = vec![10, 11, 12];

    cache.insert_with_children(1, folder, children.clone());
    let retrieved_children = cache.get_children(1);

    assert!(retrieved_children.is_some());
    assert_eq!(retrieved_children.unwrap(), children);
}

#[test]
fn test_memory_cache_get_children_roundtrip() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children = vec![100, 101, 102, 103];

    cache.insert_with_children(5, folder, children.clone());
    let retrieved = cache.get_children(5);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), children);
}

#[test]
fn test_memory_cache_get_children_ttl_expiry() {
    let cache = MemoryCache::new(Some(1));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children = vec![10, 11];

    cache.insert_with_children(1, folder, children);
    assert!(cache.get_children(1).is_some());

    std::thread::sleep(Duration::from_secs(2));
    assert!(cache.get_children(1).is_none());
}

// ============================================================================
// SQLITE STORE TESTS
// ============================================================================

#[test]
fn test_sqlite_store_open() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_open.db");
    let _ = std::fs::remove_file(&db_path);

    let _store = SqliteStore::open(&db_path)?;
    assert!(db_path.exists());

    Ok(())
}

#[test]
fn test_sqlite_store_upsert_get_item_roundtrip() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_upsert.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;
    let item = test_drive_item("item1", "test.txt", false);

    store.upsert_item(1, "drive1", &item, None)?;
    let retrieved = store.get_item_by_id("item1")?;

    assert!(retrieved.is_some());
    let (inode, retrieved_item) = retrieved.unwrap();
    assert_eq!(inode, 1);
    assert_eq!(retrieved_item.id, "item1");
    assert_eq!(retrieved_item.name, "test.txt");

    Ok(())
}

#[test]
fn test_sqlite_store_get_children() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_children.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;
    let parent = test_drive_item("parent1", "folder", true);
    let child1 = test_drive_item("child1", "file1.txt", false);
    let child2 = test_drive_item("child2", "file2.txt", false);

    store.upsert_item(1, "drive1", &parent, None)?;
    store.upsert_item(2, "drive1", &child1, Some(1))?;
    store.upsert_item(3, "drive1", &child2, Some(1))?;

    let children = store.get_children(1)?;
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].0, 2);
    assert_eq!(children[0].1.id, "child1");
    assert_eq!(children[1].0, 3);
    assert_eq!(children[1].1.id, "child2");

    Ok(())
}

#[test]
fn test_sqlite_store_delete_item() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_delete.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;
    let item = test_drive_item("item1", "test.txt", false);

    store.upsert_item(1, "drive1", &item, None)?;
    assert!(store.get_item_by_id("item1")?.is_some());

    store.delete_item("item1")?;
    assert!(store.get_item_by_id("item1")?.is_none());

    Ok(())
}

#[test]
fn test_sqlite_store_delta_tokens() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_tokens.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;

    assert!(store.get_delta_token("drive1")?.is_none());

    store.set_delta_token("drive1", "token123")?;
    let token = store.get_delta_token("drive1")?;
    assert!(token.is_some());
    assert_eq!(token.unwrap(), "token123");

    store.set_delta_token("drive1", "token456")?;
    let updated_token = store.get_delta_token("drive1")?;
    assert_eq!(updated_token.unwrap(), "token456");

    Ok(())
}

#[test]
fn test_sqlite_store_apply_delta() -> filesync_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_delta.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;

    store.set_delta_token("drive1", "token1")?;

    let item1 = test_drive_item("item1", "file1.txt", false);
    let item2 = test_drive_item("item2", "file2.txt", false);
    let item3 = test_drive_item("item3", "file3.txt", false);

    store.upsert_item(1, "drive1", &item1, None)?;
    store.upsert_item(2, "drive1", &item2, None)?;

    let items_to_add = vec![(3, item3, None)];
    let items_to_delete = vec!["item1".to_string()];

    store.apply_delta("drive1", &items_to_add, &items_to_delete, "token2")?;

    assert!(store.get_item_by_id("item1")?.is_none());
    assert!(store.get_item_by_id("item3")?.is_some());

    let token = store.get_delta_token("drive1")?;
    assert_eq!(token.unwrap(), "token2");

    Ok(())
}

// ============================================================================
// DISK CACHE TESTS
// ============================================================================

#[tokio::test]
async fn test_disk_cache_put_get_roundtrip() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_put_get");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path);
    let content = b"test file content";

    cache.put("drive1", "item1", content).await?;
    let retrieved = cache.get("drive1", "item1").await;

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), content);

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_remove() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_remove");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path);
    let content = b"test file content";

    cache.put("drive1", "item1", content).await?;
    assert!(cache.get("drive1", "item1").await.is_some());

    cache.remove("drive1", "item1").await?;
    assert!(cache.get("drive1", "item1").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_clear() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_clear");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path);

    cache.put("drive1", "item1", b"content1").await?;
    cache.put("drive1", "item2", b"content2").await?;
    assert!(cache.get("drive1", "item1").await.is_some());
    assert!(cache.get("drive1", "item2").await.is_some());

    cache.clear().await?;
    assert!(cache.get("drive1", "item1").await.is_none());
    assert!(cache.get("drive1", "item2").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_total_size() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_size");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path);

    cache.put("drive1", "item1", b"12345").await?;
    cache.put("drive1", "item2", b"1234567890").await?;

    let total = cache.total_size();
    assert_eq!(total, 15);

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_lru_eviction() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_lru");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 50, &db_path);

    cache
        .put("drive1", "item1", b"12345678901234567890")
        .await?;
    sleep(Duration::from_millis(10)).await;
    cache
        .put("drive1", "item2", b"12345678901234567890")
        .await?;
    sleep(Duration::from_millis(10)).await;
    cache
        .put("drive1", "item3", b"12345678901234567890")
        .await?;

    assert!(cache.get("drive1", "item1").await.is_none());
    assert!(cache.get("drive1", "item2").await.is_some());
    assert!(cache.get("drive1", "item3").await.is_some());

    Ok(())
}

// ============================================================================
// WRITE-BACK BUFFER TESTS
// ============================================================================

#[tokio::test]
async fn test_writeback_write_read_roundtrip() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_roundtrip");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir);
    let content = b"pending content";

    buffer.write("drive1", "item1", content).await?;
    let retrieved = buffer.read("drive1", "item1").await;

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), content);

    Ok(())
}

#[tokio::test]
async fn test_writeback_remove() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_remove");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir);
    let content = b"pending content";

    buffer.write("drive1", "item1", content).await?;
    assert!(buffer.read("drive1", "item1").await.is_some());

    buffer.remove("drive1", "item1").await?;
    assert!(buffer.read("drive1", "item1").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_writeback_list_pending() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_list");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir);

    buffer.write("drive1", "item1", b"content1").await?;
    buffer.write("drive1", "item2", b"content2").await?;
    buffer.write("drive2", "item3", b"content3").await?;

    let pending = buffer.list_pending().await?;
    assert_eq!(pending.len(), 3);

    assert!(pending.contains(&("drive1".to_string(), "item1".to_string())));
    assert!(pending.contains(&("drive1".to_string(), "item2".to_string())));
    assert!(pending.contains(&("drive2".to_string(), "item3".to_string())));

    Ok(())
}

// ============================================================================
// CRASH RECOVERY TESTS
// ============================================================================

#[tokio::test]
async fn test_writeback_crash_recovery() -> filesync_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_recovery");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    {
        let buffer = WriteBackBuffer::new(cache_dir.clone());
        buffer.write("drive1", "item1", b"content1").await?;
        buffer.write("drive1", "item2", b"content2").await?;
        buffer.write("drive2", "item3", b"content3").await?;
    }

    {
        let buffer = WriteBackBuffer::new(cache_dir);
        let pending = buffer.list_pending().await?;
        assert_eq!(pending.len(), 3);
        assert!(pending.contains(&("drive1".to_string(), "item1".to_string())));
        assert!(pending.contains(&("drive1".to_string(), "item2".to_string())));
        assert!(pending.contains(&("drive2".to_string(), "item3".to_string())));
    }

    Ok(())
}
