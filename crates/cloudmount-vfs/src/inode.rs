use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub const ROOT_INODE: u64 = 1;

struct InodeMaps {
    inode_to_item: HashMap<u64, String>,
    item_to_inode: HashMap<String, u64>,
}

pub struct InodeTable {
    next_inode: AtomicU64,
    maps: RwLock<InodeMaps>,
}

impl InodeTable {
    pub fn new() -> Self {
        Self {
            next_inode: AtomicU64::new(ROOT_INODE + 1),
            maps: RwLock::new(InodeMaps {
                inode_to_item: HashMap::new(),
                item_to_inode: HashMap::new(),
            }),
        }
    }

    /// Create a new InodeTable with the counter starting after `max_existing`.
    /// Use this when resuming from persistent storage (e.g. SQLite) to avoid
    /// inode collisions with previously persisted entries.
    pub fn new_starting_after(max_existing: u64) -> Self {
        let start = max_existing.max(ROOT_INODE) + 1;
        Self {
            next_inode: AtomicU64::new(start),
            maps: RwLock::new(InodeMaps {
                inode_to_item: HashMap::new(),
                item_to_inode: HashMap::new(),
            }),
        }
    }

    pub fn allocate(&self, item_id: &str) -> u64 {
        let mut maps = self.maps.write().expect("inode table lock poisoned");
        if let Some(&inode) = maps.item_to_inode.get(item_id) {
            return inode;
        }
        let inode = self.next_inode.fetch_add(1, Ordering::Relaxed);
        maps.inode_to_item.insert(inode, item_id.to_string());
        maps.item_to_inode.insert(item_id.to_string(), inode);
        inode
    }

    pub fn get_item_id(&self, inode: u64) -> Option<String> {
        self.maps
            .read()
            .expect("inode table lock poisoned")
            .inode_to_item
            .get(&inode)
            .cloned()
    }

    pub fn get_inode(&self, item_id: &str) -> Option<u64> {
        self.maps
            .read()
            .expect("inode table lock poisoned")
            .item_to_inode
            .get(item_id)
            .copied()
    }

    pub fn remove_by_item_id(&self, item_id: &str) {
        let mut maps = self.maps.write().expect("inode table lock poisoned");
        if let Some(inode) = maps.item_to_inode.remove(item_id) {
            maps.inode_to_item.remove(&inode);
        }
    }

    pub fn reassign(&self, inode: u64, new_item_id: &str) {
        let mut maps = self.maps.write().expect("inode table lock poisoned");
        if let Some(old_item_id) = maps.inode_to_item.get(&inode).cloned() {
            maps.item_to_inode.remove(&old_item_id);
        }
        maps.inode_to_item.insert(inode, new_item_id.to_string());
        maps.item_to_inode.insert(new_item_id.to_string(), inode);
    }

    pub fn set_root(&self, item_id: &str) {
        let mut maps = self.maps.write().expect("inode table lock poisoned");
        maps.inode_to_item.insert(ROOT_INODE, item_id.to_string());
        maps.item_to_inode.insert(item_id.to_string(), ROOT_INODE);
    }
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}
