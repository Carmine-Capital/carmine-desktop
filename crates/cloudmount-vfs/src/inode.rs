use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub const ROOT_INODE: u64 = 1;

pub struct InodeTable {
    next_inode: AtomicU64,
    inode_to_item: RwLock<HashMap<u64, String>>,
    item_to_inode: RwLock<HashMap<String, u64>>,
}

impl InodeTable {
    pub fn new() -> Self {
        Self {
            next_inode: AtomicU64::new(ROOT_INODE + 1),
            inode_to_item: RwLock::new(HashMap::new()),
            item_to_inode: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new InodeTable with the counter starting after `max_existing`.
    /// Use this when resuming from persistent storage (e.g. SQLite) to avoid
    /// inode collisions with previously persisted entries.
    pub fn new_starting_after(max_existing: u64) -> Self {
        let start = max_existing.max(ROOT_INODE) + 1;
        Self {
            next_inode: AtomicU64::new(start),
            inode_to_item: RwLock::new(HashMap::new()),
            item_to_inode: RwLock::new(HashMap::new()),
        }
    }

    pub fn allocate(&self, item_id: &str) -> u64 {
        if let Some(&inode) = self
            .item_to_inode
            .read()
            .expect("inode table lock poisoned")
            .get(item_id)
        {
            return inode;
        }

        let inode = self.next_inode.fetch_add(1, Ordering::Relaxed);
        self.inode_to_item
            .write()
            .expect("inode table lock poisoned")
            .insert(inode, item_id.to_string());
        self.item_to_inode
            .write()
            .expect("inode table lock poisoned")
            .insert(item_id.to_string(), inode);
        inode
    }

    pub fn get_item_id(&self, inode: u64) -> Option<String> {
        self.inode_to_item
            .read()
            .expect("inode table lock poisoned")
            .get(&inode)
            .cloned()
    }

    pub fn get_inode(&self, item_id: &str) -> Option<u64> {
        self.item_to_inode
            .read()
            .expect("inode table lock poisoned")
            .get(item_id)
            .copied()
    }

    pub fn remove_by_item_id(&self, item_id: &str) {
        if let Some(inode) = self
            .item_to_inode
            .write()
            .expect("inode table lock poisoned")
            .remove(item_id)
        {
            self.inode_to_item
                .write()
                .expect("inode table lock poisoned")
                .remove(&inode);
        }
    }

    pub fn reassign(&self, inode: u64, new_item_id: &str) {
        let mut i2item = self
            .inode_to_item
            .write()
            .expect("inode table lock poisoned");
        let mut item2i = self
            .item_to_inode
            .write()
            .expect("inode table lock poisoned");

        if let Some(old_item_id) = i2item.get(&inode).cloned() {
            item2i.remove(&old_item_id);
        }

        i2item.insert(inode, new_item_id.to_string());
        item2i.insert(new_item_id.to_string(), inode);
    }

    pub fn set_root(&self, item_id: &str) {
        self.inode_to_item
            .write()
            .expect("inode table lock poisoned")
            .insert(ROOT_INODE, item_id.to_string());
        self.item_to_inode
            .write()
            .expect("inode table lock poisoned")
            .insert(item_id.to_string(), ROOT_INODE);
    }
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}
