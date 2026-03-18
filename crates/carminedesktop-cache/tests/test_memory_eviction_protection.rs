use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use carminedesktop_cache::memory::MemoryCache;
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

/// Test 1: When eviction filter is set and returns true for an item,
/// maybe_evict skips that entry
#[test]
fn test_memory_eviction_filter_protects_entries() {
    let cache = MemoryCache::new(Some(60));

    // Protect items whose id starts with "protected"
    cache.set_eviction_filter(Arc::new(|item: &DriveItem| item.id.starts_with("protected")));

    // Insert more than MAX_ENTRIES (10_000) to trigger eviction
    // First insert some protected entries
    for i in 0..100 {
        let item = test_drive_item(&format!("protected-{i}"), &format!("protected-{i}"), false);
        cache.insert(i, item);
    }

    // Insert unprotected entries to fill the cache past MAX_ENTRIES
    for i in 100..10_200 {
        let item = test_drive_item(&format!("unprotected-{i}"), &format!("unprotected-{i}"), false);
        cache.insert(i, item);
    }

    // After eviction, all protected entries should still be present
    for i in 0..100 {
        assert!(
            cache.get(i).is_some(),
            "Protected entry {i} should survive eviction"
        );
    }
}

/// Test 2: When eviction filter is set, TTL expiry in get() still returns
/// cached value for protected entries (TTL check bypassed)
#[tokio::test]
async fn test_memory_ttl_bypassed_for_protected_entries_get() {
    let cache = MemoryCache::new(Some(1)); // 1 second TTL

    // Protect items whose id starts with "protected"
    cache.set_eviction_filter(Arc::new(|item: &DriveItem| item.id.starts_with("protected")));

    let protected_item = test_drive_item("protected-1", "protected-file", false);
    cache.insert(100, protected_item.clone());

    // Wait for TTL to expire
    sleep(Duration::from_secs(2)).await;

    // Protected item should still be returned despite TTL expiry
    let result = cache.get(100);
    assert!(
        result.is_some(),
        "Protected entry should be returned despite TTL expiry"
    );
    assert_eq!(result.unwrap().id, "protected-1");
}

/// Test 3: When eviction filter is set, TTL expiry in get_children() still
/// returns cached value for protected entries
#[tokio::test]
async fn test_memory_ttl_bypassed_for_protected_entries_get_children() {
    let cache = MemoryCache::new(Some(1)); // 1 second TTL

    // Protect items whose id starts with "protected"
    cache.set_eviction_filter(Arc::new(|item: &DriveItem| item.id.starts_with("protected")));

    let protected_item = test_drive_item("protected-folder", "protected-folder", true);
    let mut children = HashMap::new();
    children.insert("child.txt".to_string(), 200u64);
    cache.insert_with_children(100, protected_item, children);

    // Wait for TTL to expire
    sleep(Duration::from_secs(2)).await;

    // Protected entry should still return children despite TTL expiry
    let result = cache.get_children(100);
    assert!(
        result.is_some(),
        "Protected entry should return children despite TTL expiry"
    );
    assert_eq!(result.unwrap().len(), 1);
}

/// Test 4: When no eviction filter is set (None), eviction and TTL
/// behavior is unchanged
#[tokio::test]
async fn test_memory_ttl_works_normally_without_filter() {
    let cache = MemoryCache::new(Some(1)); // 1 second TTL

    // No filter set
    let item = test_drive_item("item-1", "file.txt", false);
    cache.insert(100, item);

    // Wait for TTL to expire
    sleep(Duration::from_secs(2)).await;

    // Item should be evicted by TTL as normal
    let result = cache.get(100);
    assert!(
        result.is_none(),
        "Unprotected expired entry should be removed by TTL"
    );
}

/// Test 5: Eviction removes enough unprotected entries to reach EVICT_TO target
#[test]
fn test_memory_eviction_removes_unprotected_to_target() {
    let cache = MemoryCache::new(Some(60));

    // Protect first 500 items
    cache.set_eviction_filter(Arc::new(|item: &DriveItem| item.id.starts_with("protected")));

    for i in 0..500 {
        let item = test_drive_item(&format!("protected-{i}"), &format!("protected-{i}"), false);
        cache.insert(i, item);
    }

    // Fill with unprotected entries past MAX_ENTRIES
    for i in 500..10_500 {
        let item = test_drive_item(&format!("unprotected-{i}"), &format!("unprotected-{i}"), false);
        cache.insert(i, item);
    }

    // All 500 protected entries should survive
    let mut protected_count = 0;
    for i in 0..500 {
        if cache.get(i).is_some() {
            protected_count += 1;
        }
    }
    assert_eq!(
        protected_count, 500,
        "All protected entries should survive eviction"
    );
}
