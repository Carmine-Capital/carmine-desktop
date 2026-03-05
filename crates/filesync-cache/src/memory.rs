use std::time::Instant;

use dashmap::DashMap;
use filesync_core::types::DriveItem;

const DEFAULT_TTL_SECS: u64 = 60;
const MAX_ENTRIES: usize = 10_000;
const EVICT_TO: usize = 8_000;

pub struct MemoryCache {
    entries: DashMap<u64, CachedEntry>,
    ttl_secs: u64,
}

struct CachedEntry {
    item: DriveItem,
    children: Option<Vec<u64>>,
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

    pub fn get_children(&self, parent_inode: u64) -> Option<Vec<u64>> {
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

    pub fn insert_with_children(&self, inode: u64, item: DriveItem, children: Vec<u64>) {
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
