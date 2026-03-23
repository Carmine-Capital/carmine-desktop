use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};
use tokio::fs;

type EvictionFilter = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

pub struct DiskCache {
    base_dir: PathBuf,
    max_size_bytes: AtomicU64,
    tracker: Mutex<Connection>,
    eviction_filter: std::sync::RwLock<Option<EvictionFilter>>,
}

impl DiskCache {
    pub fn new(
        base_dir: PathBuf,
        max_size_bytes: u64,
        db_path: &Path,
    ) -> carminedesktop_core::Result<Self> {
        let conn = Connection::open(db_path).map_err(|e| {
            carminedesktop_core::Error::Cache(format!("failed to open cache tracker db: {e}"))
        })?;
        conn.pragma_update(None, "busy_timeout", 5000).map_err(|e| {
            carminedesktop_core::Error::Cache(format!("failed to set busy_timeout: {e}"))
        })?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("failed to set tracker pragmas: {e}"))
            })?;

        // Migrate: if cache_entries was created with incompatible schema (cache_path NOT NULL),
        // drop and recreate. Tracker data is unreliable from those installs anyway.
        let has_cache_path: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('cache_entries') WHERE name = 'cache_path'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if has_cache_path {
            let _ = conn.execute_batch("DROP TABLE IF EXISTS cache_entries");
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache_entries (
                drive_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                etag TEXT,
                file_size INTEGER NOT NULL DEFAULT 0,
                last_access TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (drive_id, item_id)
            );",
        )
        .map_err(|e| {
            carminedesktop_core::Error::Cache(format!("failed to create cache_entries table: {e}"))
        })?;

        // Migration: add etag column for DBs created with old schema
        let _ = conn.execute_batch("ALTER TABLE cache_entries ADD COLUMN etag TEXT");

        Ok(Self {
            base_dir,
            max_size_bytes: AtomicU64::new(max_size_bytes),
            tracker: Mutex::new(conn),
            eviction_filter: std::sync::RwLock::new(None),
        })
    }

    fn content_path(&self, drive_id: &str, item_id: &str) -> PathBuf {
        // Sanitize colons — illegal in Windows filenames (e.g. "local:uuid" items)
        self.base_dir
            .join(drive_id)
            .join(item_id.replace(':', "%3A"))
    }

    pub async fn get(&self, drive_id: &str, item_id: &str) -> Option<Vec<u8>> {
        let path = self.content_path(drive_id, item_id);
        let data = fs::read(&path).await.ok()?;
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "UPDATE cache_entries SET last_access = datetime('now') WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
            );
        }
        Some(data)
    }

    /// Read a byte range from a cached file without loading the entire content.
    pub fn get_range(
        &self,
        drive_id: &str,
        item_id: &str,
        offset: u64,
        length: u64,
    ) -> Option<Vec<u8>> {
        let path = self.content_path(drive_id, item_id);
        let mut file = std::fs::File::open(&path).ok()?;
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; length as usize];
        match file.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // File shorter than requested range — read what's available
                let metadata = std::fs::metadata(&path).ok()?;
                let file_len = metadata.len();
                if offset >= file_len {
                    return Some(Vec::new());
                }
                let available = (file_len - offset) as usize;
                buf.truncate(available);
                let mut file = std::fs::File::open(&path).ok()?;
                file.seek(SeekFrom::Start(offset)).ok()?;
                file.read_exact(&mut buf).ok()?;
            }
            Err(_) => return None,
        }
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "UPDATE cache_entries SET last_access = datetime('now') WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
            );
        }
        Some(buf)
    }

    pub async fn get_with_etag(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> Option<(Vec<u8>, Option<String>)> {
        let path = self.content_path(drive_id, item_id);
        let data = fs::read(&path).await.ok()?;
        let etag = if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "UPDATE cache_entries SET last_access = datetime('now') WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
            );
            conn.query_row(
                "SELECT etag FROM cache_entries WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap_or(None)
        } else {
            None
        };
        Some((data, etag))
    }

    pub async fn put(
        &self,
        drive_id: &str,
        item_id: &str,
        content: &[u8],
        etag: Option<&str>,
    ) -> carminedesktop_core::Result<()> {
        let path = self.content_path(drive_id, item_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| carminedesktop_core::Error::Cache(format!("mkdir failed: {e}")))?;
        }
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, content)
            .await
            .map_err(|e| carminedesktop_core::Error::Cache(format!("write cache failed: {e}")))?;
        fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| carminedesktop_core::Error::Cache(format!("rename cache failed: {e}")))?;

        let file_size = content.len() as i64;
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "INSERT INTO cache_entries (drive_id, item_id, file_size, etag, last_access)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(drive_id, item_id) DO UPDATE SET
                    file_size = excluded.file_size,
                    etag = excluded.etag,
                    last_access = datetime('now')",
                params![drive_id, item_id, file_size, etag],
            );
        }

        self.evict_if_needed().await?;

        Ok(())
    }

    pub async fn remove(&self, drive_id: &str, item_id: &str) -> carminedesktop_core::Result<()> {
        let path = self.content_path(drive_id, item_id);
        match fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(carminedesktop_core::Error::Cache(format!(
                    "remove cache failed: {e}"
                )));
            }
        }
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "DELETE FROM cache_entries WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
            );
        }
        Ok(())
    }

    pub async fn clear(&self) -> carminedesktop_core::Result<()> {
        match fs::remove_dir_all(&self.base_dir).await {
            Ok(()) => {
                fs::create_dir_all(&self.base_dir).await.map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("recreate cache dir failed: {e}"))
                })?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(carminedesktop_core::Error::Cache(format!(
                    "clear cache failed: {e}"
                )));
            }
        }
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute("DELETE FROM cache_entries", []);
        }
        Ok(())
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn max_size_bytes(&self) -> u64 {
        self.max_size_bytes.load(Ordering::Relaxed)
    }

    pub fn set_max_size(&self, max_size_bytes: u64) {
        self.max_size_bytes.store(max_size_bytes, Ordering::Relaxed);
    }

    /// Set a filter predicate for eviction.  If the filter returns `true`
    /// for a (drive_id, item_id) pair, that entry is skipped during LRU
    /// eviction (it is "protected").
    pub fn set_eviction_filter(&self, filter: EvictionFilter) {
        *self.eviction_filter.write().unwrap() = Some(filter);
    }

    pub fn total_size(&self) -> u64 {
        let conn = match self.tracker.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM cache_entries",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64
    }

    /// Return the number of entries tracked in the cache.
    pub fn entry_count(&self) -> u64 {
        let conn = match self.tracker.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM cache_entries", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0) as u64
    }

    async fn evict_if_needed(&self) -> carminedesktop_core::Result<()> {
        let max = self.max_size_bytes.load(Ordering::Relaxed);
        if max == 0 {
            return Ok(());
        }

        let total = self.total_size();
        if total <= max {
            return Ok(());
        }

        let to_free = total - max;
        let entries = {
            let conn = self.tracker.lock().map_err(|e| {
                carminedesktop_core::Error::Cache(format!("tracker lock failed: {e}"))
            })?;
            let mut stmt = conn
                .prepare(
                    "SELECT drive_id, item_id, file_size FROM cache_entries ORDER BY last_access ASC",
                )
                .map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("prepare eviction query failed: {e}"))
                })?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("eviction query failed: {e}"))
                })?;

            let entries: Vec<_> = rows.flatten().collect();
            entries
        };

        let filter = self.eviction_filter.read().unwrap().clone();
        let mut freed: u64 = 0;
        for (drive_id, item_id, size) in entries {
            if freed >= to_free {
                break;
            }
            // Skip protected (pinned) entries
            if let Some(ref f) = filter
                && f(&drive_id, &item_id)
            {
                continue;
            }
            let path = self.content_path(&drive_id, &item_id);
            let _ = fs::remove_file(&path).await;
            if let Ok(conn) = self.tracker.lock() {
                let _ = conn.execute(
                    "DELETE FROM cache_entries WHERE drive_id = ?1 AND item_id = ?2",
                    params![drive_id, item_id],
                );
            }
            freed += size as u64;
            tracing::debug!("evicted cache entry {drive_id}/{item_id} ({size} bytes)");
        }

        if freed < to_free {
            tracing::warn!(
                "cache eviction could not free enough space: freed {freed} bytes, \
                 target was {to_free} (some entries may be protected by offline pins)"
            );
        }

        tracing::info!("cache eviction freed {freed} bytes (target was {to_free})");
        Ok(())
    }
}
