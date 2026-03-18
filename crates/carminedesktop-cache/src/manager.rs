use std::path::PathBuf;
use std::sync::Arc;

use carminedesktop_core::types::DriveItem;
use dashmap::DashSet;

use crate::disk::DiskCache;
use crate::memory::MemoryCache;
use crate::pin_store::PinStore;
use crate::sqlite::SqliteStore;
use crate::writeback::WriteBackBuffer;

pub struct CacheManager {
    pub memory: MemoryCache,
    pub sqlite: SqliteStore,
    pub disk: DiskCache,
    pub writeback: WriteBackBuffer,
    /// Inodes known to have stale content (set by delta sync, checked by open_file).
    pub dirty_inodes: DashSet<u64>,
    pub pin_store: Arc<PinStore>,
}

impl CacheManager {
    pub fn new(
        cache_dir: PathBuf,
        db_path: PathBuf,
        max_cache_bytes: u64,
        ttl_secs: Option<u64>,
        drive_id: String,
    ) -> carminedesktop_core::Result<Self> {
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            carminedesktop_core::Error::Cache(format!("create cache dir failed: {e}"))
        })?;

        let sqlite = SqliteStore::open(&db_path)?;
        let memory = MemoryCache::new(ttl_secs);
        let disk = DiskCache::new(cache_dir.join("content"), max_cache_bytes, &db_path)?;
        let writeback = WriteBackBuffer::new(cache_dir);
        let pin_store = Arc::new(PinStore::open(&db_path)?);

        // Wire disk eviction protection: items in pinned folder trees are never evicted
        let ps = pin_store.clone();
        disk.set_eviction_filter(Arc::new(move |did: &str, item_id: &str| {
            ps.is_protected(did, item_id)
        }));

        // Wire memory cache eviction protection: pinned items never evicted or TTL-expired
        let ps2 = pin_store.clone();
        let drive_id_owned = drive_id;
        memory.set_eviction_filter(Arc::new(move |item: &DriveItem| {
            ps2.is_protected(&drive_id_owned, &item.id)
        }));

        Ok(Self {
            memory,
            sqlite,
            disk,
            writeback,
            dirty_inodes: DashSet::new(),
            pin_store,
        })
    }

    pub async fn clear(&self) -> carminedesktop_core::Result<()> {
        self.memory.clear();
        self.sqlite.clear()?;
        self.disk.clear().await?;
        self.dirty_inodes.clear();
        Ok(())
    }
}
