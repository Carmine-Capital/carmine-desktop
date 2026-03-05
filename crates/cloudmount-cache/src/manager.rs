use std::path::PathBuf;

use crate::disk::DiskCache;
use crate::memory::MemoryCache;
use crate::sqlite::SqliteStore;
use crate::writeback::WriteBackBuffer;

pub struct CacheManager {
    pub memory: MemoryCache,
    pub sqlite: SqliteStore,
    pub disk: DiskCache,
    pub writeback: WriteBackBuffer,
}

impl CacheManager {
    pub fn new(
        cache_dir: PathBuf,
        db_path: PathBuf,
        max_cache_bytes: u64,
        ttl_secs: Option<u64>,
    ) -> cloudmount_core::Result<Self> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| cloudmount_core::Error::Cache(format!("create cache dir failed: {e}")))?;

        let sqlite = SqliteStore::open(&db_path)?;
        let memory = MemoryCache::new(ttl_secs);
        let disk = DiskCache::new(cache_dir.join("content"), max_cache_bytes, &db_path);
        let writeback = WriteBackBuffer::new(cache_dir);

        Ok(Self {
            memory,
            sqlite,
            disk,
            writeback,
        })
    }
}
