use std::collections::HashMap;
use std::time::Instant;

use cloudmount_core::types::DriveItem;
use dashmap::DashMap;

const DEFAULT_TTL_SECS: u64 = 60;
const MAX_ENTRIES: usize = 10_000;
const EVICT_TO: usize = 8_000;

pub struct MemoryCache {
    entries: DashMap<u64, CachedEntry>,
    ttl_secs: u64,
}

struct CachedEntry {
    item: DriveItem,
    children: Option<HashMap<String, u64>>,
    inserted_at: Instant,
    last_access: Instant,
}

impl MemoryCache {
    pub fn new(ttl_secs: Option<u64>) -> Self {
        Self {
            entries: DashMap::new(),
            ttl_secs: ttl_secs.unwrap_or(DEFAULT_TTL_SECS),
        }
    }

    pub fn get(&self, inode: u64) -> Option<DriveItem> {
        let mut entry = self.entries.get_mut(&inode)?;
        let elapsed = entry.inserted_at.elapsed().as_secs();
        if elapsed > self.ttl_secs {
            drop(entry);
            self.entries.remove(&inode);
            return None;
        }
        entry.last_access = Instant::now();
        Some(entry.item.clone())
    }

    pub fn get_children(&self, parent_inode: u64) -> Option<HashMap<String, u64>> {
        let mut entry = self.entries.get_mut(&parent_inode)?;
        let elapsed = entry.inserted_at.elapsed().as_secs();
        if elapsed > self.ttl_secs {
            return None;
        }
        entry.last_access = Instant::now();
        entry.children.clone()
    }

    pub fn insert(&self, inode: u64, item: DriveItem) {
        self.maybe_evict();
        let now = Instant::now();
        self.entries.insert(
            inode,
            CachedEntry {
                item,
                children: None,
                inserted_at: now,
                last_access: now,
            },
        );
    }

    pub fn insert_with_children(
        &self,
        inode: u64,
        item: DriveItem,
        children: HashMap<String, u64>,
    ) {
        self.maybe_evict();
        let now = Instant::now();
        self.entries.insert(
            inode,
            CachedEntry {
                item,
                children: Some(children),
                inserted_at: now,
                last_access: now,
            },
        );
    }

    pub fn add_child(&self, parent_inode: u64, name: &str, child_inode: u64) {
        if let Some(mut entry) = self.entries.get_mut(&parent_inode)
            && let Some(children) = &mut entry.children
        {
            // On Windows, remove any case-insensitive duplicate before inserting
            // so that a rename like "file.txt" → "FILE.TXT" doesn't leave a
            // stale entry under the old case.
            #[cfg(target_os = "windows")]
            {
                let dup = children
                    .keys()
                    .find(|k| k.eq_ignore_ascii_case(name) && k.as_str() != name)
                    .cloned();
                if let Some(dup) = dup {
                    children.remove(&dup);
                }
            }
            children.insert(name.to_string(), child_inode);
        }
    }

    pub fn remove_child(&self, parent_inode: u64, name: &str) {
        if let Some(mut entry) = self.entries.get_mut(&parent_inode)
            && let Some(children) = &mut entry.children
        {
            // On Windows, filenames are case-insensitive so the stored key may
            // differ in case from the name provided by the OS.  Fall back to a
            // case-insensitive scan when the exact key is missing.
            #[cfg(target_os = "windows")]
            if !children.contains_key(name) {
                let key = children
                    .keys()
                    .find(|k| k.eq_ignore_ascii_case(name))
                    .cloned();
                if let Some(key) = key {
                    children.remove(&key);
                    return;
                }
            }
            children.remove(name);
        }
    }

    pub fn invalidate(&self, inode: u64) {
        self.entries.remove(&inode);
    }

    pub fn clear(&self) {
        self.entries.clear();
    }

    fn maybe_evict(&self) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }

        let mut entries: Vec<(u64, Instant)> = self
            .entries
            .iter()
            .map(|e| (*e.key(), e.value().last_access))
            .collect();

        entries.sort_by_key(|(_, t)| *t);

        let to_remove = entries.len() - EVICT_TO;
        for (inode, _) in entries.into_iter().take(to_remove) {
            self.entries.remove(&inode);
        }
    }
}
