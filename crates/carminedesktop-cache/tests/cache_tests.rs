use std::collections::HashMap;
use std::time::Duration;

use carminedesktop_cache::{
    disk::DiskCache, memory::MemoryCache, sqlite::SqliteStore, writeback::WriteBackBuffer,
};
use carminedesktop_core::types::{DriveItem, FolderFacet};
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
        publication: None,
        download_url: None,
        web_url: None,
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
    let children: HashMap<String, u64> = HashMap::from([
        ("a.txt".into(), 10),
        ("b.txt".into(), 11),
        ("c.txt".into(), 12),
    ]);

    cache.insert_with_children(1, folder, children.clone());
    let retrieved_children = cache.get_children(1);

    assert!(retrieved_children.is_some());
    assert_eq!(retrieved_children.unwrap(), children);
}

#[test]
fn test_memory_cache_get_children_roundtrip() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> = HashMap::from([
        ("w.txt".into(), 100),
        ("x.txt".into(), 101),
        ("y.txt".into(), 102),
        ("z.txt".into(), 103),
    ]);

    cache.insert_with_children(5, folder, children.clone());
    let retrieved = cache.get_children(5);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), children);
}

#[test]
fn test_memory_cache_get_children_ttl_expiry() {
    let cache = MemoryCache::new(Some(1));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> =
        HashMap::from([("a.txt".into(), 10), ("b.txt".into(), 11)]);

    cache.insert_with_children(1, folder, children);
    assert!(cache.get_children(1).is_some());

    std::thread::sleep(Duration::from_secs(2));
    assert!(cache.get_children(1).is_none());
}

#[test]
fn test_memory_cache_add_child() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> = HashMap::from([("a.txt".into(), 10)]);

    cache.insert_with_children(1, folder, children);
    cache.add_child(1, "b.txt", 11);

    let retrieved = cache.get_children(1).unwrap();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved["a.txt"], 10);
    assert_eq!(retrieved["b.txt"], 11);
}

#[test]
fn test_memory_cache_add_child_noop_when_not_cached() {
    let cache = MemoryCache::new(Some(60));
    // Parent not in cache — should be a no-op
    cache.add_child(999, "file.txt", 10);
    assert!(cache.get_children(999).is_none());
}

#[test]
fn test_memory_cache_add_child_noop_when_children_none() {
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item);
    cache.add_child(1, "file.txt", 10);
    // children was None, so add_child is a no-op
    assert!(cache.get_children(1).is_none());
}

#[test]
fn test_memory_cache_remove_child() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> =
        HashMap::from([("a.txt".into(), 10), ("b.txt".into(), 11)]);

    cache.insert_with_children(1, folder, children);
    cache.remove_child(1, "a.txt");

    let retrieved = cache.get_children(1).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved["b.txt"], 11);
}

#[test]
fn test_memory_cache_remove_child_noop_when_not_cached() {
    let cache = MemoryCache::new(Some(60));
    cache.remove_child(999, "file.txt");
    // No panic, no-op
}

#[test]
fn test_memory_cache_remove_child_noop_when_children_none() {
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item);
    cache.remove_child(1, "file.txt");
    // No panic, no-op
}

#[test]
fn test_memory_cache_remove_child_nonexistent_name() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> = HashMap::from([("a.txt".into(), 10)]);

    cache.insert_with_children(1, folder, children);
    cache.remove_child(1, "nonexistent.txt");

    let retrieved = cache.get_children(1).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved["a.txt"], 10);
}

// ============================================================================
// SQLITE STORE TESTS
// ============================================================================

#[test]
fn test_sqlite_store_open() -> carminedesktop_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_open.db");
    let _ = std::fs::remove_file(&db_path);

    let _store = SqliteStore::open(&db_path)?;
    assert!(db_path.exists());

    Ok(())
}

#[test]
fn test_sqlite_store_upsert_get_item_roundtrip() -> carminedesktop_core::Result<()> {
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
fn test_sqlite_store_get_children() -> carminedesktop_core::Result<()> {
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
fn test_sqlite_store_delete_item() -> carminedesktop_core::Result<()> {
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
fn test_sqlite_store_delta_tokens() -> carminedesktop_core::Result<()> {
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
fn test_sqlite_store_apply_delta() -> carminedesktop_core::Result<()> {
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
async fn test_disk_cache_put_get_roundtrip() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_put_get");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;
    let content = b"test file content";

    cache.put("drive1", "item1", content, None).await?;
    let retrieved = cache.get("drive1", "item1").await;

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), content);

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_remove() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_remove");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;
    let content = b"test file content";

    cache.put("drive1", "item1", content, None).await?;
    assert!(cache.get("drive1", "item1").await.is_some());

    cache.remove("drive1", "item1").await?;
    assert!(cache.get("drive1", "item1").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_clear() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_clear");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;

    cache.put("drive1", "item1", b"content1", None).await?;
    cache.put("drive1", "item2", b"content2", None).await?;
    assert!(cache.get("drive1", "item1").await.is_some());
    assert!(cache.get("drive1", "item2").await.is_some());

    cache.clear().await?;
    assert!(cache.get("drive1", "item1").await.is_none());
    assert!(cache.get("drive1", "item2").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_total_size() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_size");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;

    cache.put("drive1", "item1", b"12345", None).await?;
    cache.put("drive1", "item2", b"1234567890", None).await?;

    let total = cache.total_size();
    assert_eq!(total, 15);

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_lru_eviction() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_cache_lru");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 50, &db_path)?;

    cache
        .put("drive1", "item1", b"12345678901234567890", None)
        .await?;
    sleep(Duration::from_millis(10)).await;
    cache
        .put("drive1", "item2", b"12345678901234567890", None)
        .await?;
    sleep(Duration::from_millis(10)).await;
    cache
        .put("drive1", "item3", b"12345678901234567890", None)
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
async fn test_writeback_write_read_roundtrip() -> carminedesktop_core::Result<()> {
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
async fn test_writeback_remove() -> carminedesktop_core::Result<()> {
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
async fn test_writeback_list_pending() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_list");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir);

    buffer.write("drive1", "item1", b"content1").await?;
    buffer.persist("drive1", "item1").await?;
    buffer.write("drive1", "item2", b"content2").await?;
    buffer.persist("drive1", "item2").await?;
    buffer.write("drive2", "item3", b"content3").await?;
    buffer.persist("drive2", "item3").await?;

    let pending = buffer.list_pending().await?;
    assert_eq!(pending.len(), 3);

    assert!(pending.contains(&("drive1".to_string(), "item1".to_string())));
    assert!(pending.contains(&("drive1".to_string(), "item2".to_string())));
    assert!(pending.contains(&("drive2".to_string(), "item3".to_string())));

    Ok(())
}

#[tokio::test]
async fn test_writeback_write_persists_to_disk_immediately() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_persist_on_write");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir.clone());
    let content = b"crash-safe content";

    // write() should persist to disk immediately (Fix 5)
    buffer.write("drive1", "item1", content).await?;

    // Verify the file exists on disk without calling persist() explicitly
    let pending_path = cache_dir.join("pending").join("drive1").join("item1");
    assert!(
        pending_path.exists(),
        "write() should persist to disk immediately for crash safety"
    );

    // Verify content matches
    let disk_content = tokio::fs::read(&pending_path).await?;
    assert_eq!(disk_content, content);

    // A new buffer instance (simulating restart) should be able to read the content
    let buffer2 = WriteBackBuffer::new(cache_dir);
    let recovered = buffer2.read("drive1", "item1").await;
    assert!(
        recovered.is_some(),
        "content should survive process restart"
    );
    assert_eq!(recovered.unwrap(), content);

    Ok(())
}

#[tokio::test]
async fn test_writeback_local_colon_id_roundtrips() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_colon_id");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let buffer = WriteBackBuffer::new(cache_dir.clone());
    let content = b"local file content";
    let local_id = "local:1709913612345678";

    // Write with a colon-containing item_id (illegal filename char on Windows)
    buffer.write("drive1", local_id, content).await?;

    // list_pending should return the original unsanitized item_id
    let pending = buffer.list_pending().await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0], ("drive1".to_string(), local_id.to_string()));

    // read should find the content by original item_id
    let data = buffer.read("drive1", local_id).await;
    assert_eq!(data.as_deref(), Some(content.as_slice()));

    // remove should work by original item_id
    buffer.remove("drive1", local_id).await?;
    let pending = buffer.list_pending().await?;
    assert!(pending.is_empty());

    Ok(())
}

// ============================================================================
// DISK CACHE ETAG TRACKING TESTS
// ============================================================================

#[tokio::test]
async fn test_disk_cache_put_with_etag_get_with_etag_returns_it() -> carminedesktop_core::Result<()>
{
    let cache_dir = std::env::temp_dir().join("test_disk_etag_put_get");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;
    let content = b"test file content";

    cache
        .put("drive1", "item1", content, Some("etag-abc"))
        .await?;
    let result = cache.get_with_etag("drive1", "item1").await;

    assert!(result.is_some());
    let (data, etag) = result.unwrap();
    assert_eq!(data, content);
    assert_eq!(etag, Some("etag-abc".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_put_without_etag_get_with_etag_returns_none()
-> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_etag_none");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;

    cache.put("drive1", "item1", b"content", None).await?;
    let result = cache.get_with_etag("drive1", "item1").await;

    assert!(result.is_some());
    let (_, etag) = result.unwrap();
    assert!(etag.is_none());

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_etag_updated_on_reput() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_etag_update");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;

    cache.put("drive1", "item1", b"v1", Some("etag-1")).await?;
    cache.put("drive1", "item1", b"v2", Some("etag-2")).await?;

    let (data, etag) = cache.get_with_etag("drive1", "item1").await.unwrap();
    assert_eq!(data, b"v2");
    assert_eq!(etag, Some("etag-2".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_disk_cache_schema_migration_adds_etag_column() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_disk_etag_migration");
    let db_path = cache_dir.join("tracker.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    // Simulate old schema without etag column
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE cache_entries (
                drive_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                last_access TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (drive_id, item_id)
            );",
        )
        .unwrap();
        // Insert an entry without etag
        conn.execute(
            "INSERT INTO cache_entries (drive_id, item_id, file_size) VALUES ('d1', 'i1', 100)",
            [],
        )
        .unwrap();
    }

    // Open DiskCache which should migrate
    let cache = DiskCache::new(cache_dir.join("content"), 1_000_000, &db_path)?;

    // Should be able to put with etag and get it back
    cache
        .put("drive1", "item2", b"new content", Some("etag-new"))
        .await?;
    let result = cache.get_with_etag("drive1", "item2").await;
    assert!(result.is_some());
    let (_, etag) = result.unwrap();
    assert_eq!(etag, Some("etag-new".to_string()));

    Ok(())
}

// ============================================================================
// DIRTY INODES SET TESTS
// ============================================================================

#[test]
fn test_dirty_inodes_mark_and_check() {
    let cache_dir = std::env::temp_dir().join("test_dirty_inodes_basic");
    let db_path = cache_dir.join("metadata.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache =
        carminedesktop_cache::CacheManager::new(cache_dir, db_path, 1_000_000, Some(60)).unwrap();

    assert!(!cache.dirty_inodes.contains(&42));
    cache.dirty_inodes.insert(42);
    assert!(cache.dirty_inodes.contains(&42));
    cache.dirty_inodes.remove(&42);
    assert!(!cache.dirty_inodes.contains(&42));
}

#[test]
fn test_dirty_inodes_concurrent_access() {
    let cache_dir = std::env::temp_dir().join("test_dirty_inodes_concurrent");
    let db_path = cache_dir.join("metadata.db");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = std::sync::Arc::new(
        carminedesktop_cache::CacheManager::new(cache_dir, db_path, 1_000_000, Some(60)).unwrap(),
    );

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let cache = cache.clone();
            std::thread::spawn(move || {
                for j in 0..100 {
                    let ino = i * 100 + j;
                    cache.dirty_inodes.insert(ino);
                    assert!(cache.dirty_inodes.contains(&ino));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(cache.dirty_inodes.len(), 1000);
}

// ============================================================================
// CRASH RECOVERY TESTS
// ============================================================================

#[tokio::test]
async fn test_writeback_crash_recovery() -> carminedesktop_core::Result<()> {
    let cache_dir = std::env::temp_dir().join("test_writeback_recovery");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir)?;

    {
        let buffer = WriteBackBuffer::new(cache_dir.clone());
        buffer.write("drive1", "item1", b"content1").await?;
        buffer.persist("drive1", "item1").await?;
        buffer.write("drive1", "item2", b"content2").await?;
        buffer.persist("drive1", "item2").await?;
        buffer.write("drive2", "item3", b"content3").await?;
        buffer.persist("drive2", "item3").await?;
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

// ============================================================================
// PATH RESOLUTION TESTS
// ============================================================================

#[test]
fn test_sync_resolve_relative_path_nested() {
    use carminedesktop_cache::resolve_relative_path;
    use carminedesktop_core::types::ParentReference;
    use std::path::PathBuf;

    let item = DriveItem {
        id: "item1".to_string(),
        name: "quarterly.xlsx".to_string(),
        size: 1024,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: Some(ParentReference {
            drive_id: None,
            id: None,
            path: Some("/drive/root:/Documents/Reports".to_string()),
        }),
        folder: None,
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    };

    let result = resolve_relative_path(&item);
    assert_eq!(
        result,
        Some(PathBuf::from("Documents/Reports/quarterly.xlsx"))
    );
}

#[test]
fn test_sync_resolve_relative_path_root_level() {
    use carminedesktop_cache::resolve_relative_path;
    use carminedesktop_core::types::ParentReference;
    use std::path::PathBuf;

    let item = DriveItem {
        id: "item2".to_string(),
        name: "readme.txt".to_string(),
        size: 256,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: Some(ParentReference {
            drive_id: None,
            id: None,
            path: Some("/drive/root:".to_string()),
        }),
        folder: None,
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    };

    let result = resolve_relative_path(&item);
    assert_eq!(result, Some(PathBuf::from("readme.txt")));
}

#[test]
fn test_sync_resolve_relative_path_missing_parent_reference() {
    use carminedesktop_cache::resolve_relative_path;

    let item = DriveItem {
        id: "item3".to_string(),
        name: "orphan.txt".to_string(),
        size: 100,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: None,
        folder: None,
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    };

    let result = resolve_relative_path(&item);
    assert_eq!(result, None);
}

#[test]
fn test_sync_resolve_relative_path_drives_prefix() {
    use carminedesktop_cache::resolve_relative_path;
    use carminedesktop_core::types::ParentReference;
    use std::path::PathBuf;

    let item = DriveItem {
        id: "item4".to_string(),
        name: "report.pdf".to_string(),
        size: 2048,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: Some(ParentReference {
            drive_id: Some("b!abc123".to_string()),
            id: None,
            path: Some("/drives/b!abc123/root:/Shared Documents".to_string()),
        }),
        folder: None,
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    };

    let result = resolve_relative_path(&item);
    assert_eq!(result, Some(PathBuf::from("Shared Documents/report.pdf")));
}

#[test]
fn test_sync_resolve_deleted_path_standard() {
    use carminedesktop_cache::DeletedItemInfo;
    use carminedesktop_cache::resolve_deleted_path;
    use std::path::PathBuf;

    let info = DeletedItemInfo {
        id: "del1".to_string(),
        name: "old_file.txt".to_string(),
        parent_path: Some("/drive/root:/Archive".to_string()),
    };

    let result = resolve_deleted_path(&info);
    assert_eq!(result, Some(PathBuf::from("Archive/old_file.txt")));
}

#[test]
fn test_sync_resolve_deleted_path_empty_name() {
    use carminedesktop_cache::DeletedItemInfo;
    use carminedesktop_cache::resolve_deleted_path;

    let info = DeletedItemInfo {
        id: "del2".to_string(),
        name: String::new(),
        parent_path: Some("/drive/root:/Something".to_string()),
    };

    let result = resolve_deleted_path(&info);
    assert_eq!(result, None);
}

#[test]
fn test_sync_resolve_deleted_path_missing_parent() {
    use carminedesktop_cache::DeletedItemInfo;
    use carminedesktop_cache::resolve_deleted_path;

    let info = DeletedItemInfo {
        id: "del3".to_string(),
        name: "file.txt".to_string(),
        parent_path: None,
    };

    let result = resolve_deleted_path(&info);
    assert_eq!(result, None);
}

// ============================================================================
// WRITEBACK HAS_PENDING TESTS
// ============================================================================

#[tokio::test]
async fn test_writeback_has_pending_in_memory() -> carminedesktop_core::Result<()> {
    let base = std::env::temp_dir().join("carminedesktop_test_has_pending_mem");
    let _ = std::fs::remove_dir_all(&base);

    let wb = WriteBackBuffer::new(base.clone());
    assert!(!wb.has_pending("drive1", "item1"));

    wb.write("drive1", "item1", b"content").await?;
    assert!(wb.has_pending("drive1", "item1"));
    assert!(!wb.has_pending("drive1", "item_other"));

    wb.remove("drive1", "item1").await?;
    assert!(!wb.has_pending("drive1", "item1"));

    let _ = std::fs::remove_dir_all(&base);
    Ok(())
}

#[tokio::test]
async fn test_writeback_has_pending_on_disk_only() -> carminedesktop_core::Result<()> {
    let base = std::env::temp_dir().join("carminedesktop_test_has_pending_disk");
    let _ = std::fs::remove_dir_all(&base);

    // Write via chunked write (bypasses in-memory buffer)
    let wb = WriteBackBuffer::new(base.clone());
    wb.write_chunk("drive1", "item1", 0, b"chunk data").await?;
    wb.finish_chunked_write("drive1", "item1").await?;

    // In-memory buffer is empty, but disk file exists
    assert!(wb.has_pending("drive1", "item1"));
    assert!(!wb.has_pending("drive1", "nonexistent"));

    let _ = std::fs::remove_dir_all(&base);
    Ok(())
}

// ============================================================================
// DELTA SYNC OBSERVER TESTS
// ============================================================================

use std::sync::atomic::{AtomicU64, Ordering};

/// Mock delta sync observer that tracks which inodes were notified.
struct MockDeltaSyncObserver {
    last_ino: AtomicU64,
    call_count: AtomicU64,
}

impl MockDeltaSyncObserver {
    fn new() -> Self {
        Self {
            last_ino: AtomicU64::new(0),
            call_count: AtomicU64::new(0),
        }
    }
}

impl carminedesktop_core::DeltaSyncObserver for MockDeltaSyncObserver {
    fn on_inode_content_changed(&self, ino: u64) {
        self.last_ino.store(ino, Ordering::Relaxed);
        self.call_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[tokio::test]
async fn test_delta_sync_observer_called_on_etag_change() {
    use carminedesktop_cache::CacheManager;
    use carminedesktop_cache::sync::run_delta_sync;
    use carminedesktop_core::types::{DriveItem, FileFacet, ParentReference};
    use std::sync::Arc;

    let server = wiremock::MockServer::start().await;

    let drive_id = "test-drive";
    let item_id = "file-1";

    let base = std::env::temp_dir().join(format!(
        "carminedesktop-observer-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    let graph = Arc::new(carminedesktop_graph::GraphClient::with_base_url(
        server.uri(),
        || async { Ok("test-token".to_string()) },
    ));

    // Pre-populate SQLite with an existing item that has a different eTag
    let existing_item = DriveItem {
        id: item_id.to_string(),
        name: "hello.txt".to_string(),
        size: 13,
        last_modified: None,
        created: None,
        etag: Some("etag-old".to_string()),
        parent_reference: Some(ParentReference {
            drive_id: Some(drive_id.to_string()),
            id: Some("root-id".to_string()),
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
        .upsert_item(2, drive_id, &existing_item, Some(1))
        .unwrap();

    // Mock delta response with a changed eTag
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/root/delta"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{
                    "id": item_id,
                    "name": "hello.txt",
                    "size": 7000,
                    "eTag": "etag-new",
                    "parentReference": { "driveId": drive_id, "id": "root-id" },
                    "file": { "mimeType": "text/plain" }
                }],
                "@odata.deltaLink": "https://example.com/delta?token=new"
            })),
        )
        .mount(&server)
        .await;

    let observer = Arc::new(MockDeltaSyncObserver::new());
    let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        Arc::new(|id: &str| if id == "file-1" { 2 } else { 1 });

    run_delta_sync(
        &graph,
        &cache,
        drive_id,
        &inode_allocator,
        Some(observer.as_ref()),
    )
    .await
    .unwrap();

    assert_eq!(
        observer.call_count.load(Ordering::Relaxed),
        1,
        "observer should be called once for the changed item"
    );
    assert_eq!(
        observer.last_ino.load(Ordering::Relaxed),
        2,
        "observer should be called with the correct inode"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_delta_sync_no_observer_still_works() {
    use carminedesktop_cache::CacheManager;
    use carminedesktop_cache::sync::run_delta_sync;
    use std::sync::Arc;

    let server = wiremock::MockServer::start().await;
    let drive_id = "test-drive";

    let base = std::env::temp_dir().join(format!(
        "carminedesktop-no-observer-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    let graph = Arc::new(carminedesktop_graph::GraphClient::with_base_url(
        server.uri(),
        || async { Ok("test-token".to_string()) },
    ));

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/drives/{drive_id}/root/delta"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [],
                "@odata.deltaLink": "https://example.com/delta?token=new"
            })),
        )
        .mount(&server)
        .await;

    let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> = Arc::new(|_: &str| 1);

    // Should succeed without observer (None)
    let result = run_delta_sync(&graph, &cache, drive_id, &inode_allocator, None).await;
    assert!(result.is_ok(), "delta sync should work without observer");

    let _ = std::fs::remove_dir_all(&base);
}
