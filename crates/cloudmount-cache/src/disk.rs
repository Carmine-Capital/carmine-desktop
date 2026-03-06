use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, params};
use tokio::fs;

pub struct DiskCache {
    base_dir: PathBuf,
    max_size_bytes: AtomicU64,
    tracker: Mutex<Connection>,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf, max_size_bytes: u64, db_path: &Path) -> Self {
        let conn = Connection::open(db_path).expect("failed to open cache tracker db");
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .expect("failed to set tracker pragmas");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache_entries (
                drive_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                last_access TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (drive_id, item_id)
            );",
        )
        .expect("failed to create cache_entries table");

        Self {
            base_dir,
            max_size_bytes: AtomicU64::new(max_size_bytes),
            tracker: Mutex::new(conn),
        }
    }

    fn content_path(&self, drive_id: &str, item_id: &str) -> PathBuf {
        self.base_dir.join(drive_id).join(item_id)
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

    pub async fn put(
        &self,
        drive_id: &str,
        item_id: &str,
        content: &[u8],
    ) -> cloudmount_core::Result<()> {
        let path = self.content_path(drive_id, item_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("mkdir failed: {e}")))?;
        }
        fs::write(&path, content)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("write cache failed: {e}")))?;

        let file_size = content.len() as i64;
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "INSERT INTO cache_entries (drive_id, item_id, file_size, last_access)
                 VALUES (?1, ?2, ?3, datetime('now'))
                 ON CONFLICT(drive_id, item_id) DO UPDATE SET
                    file_size = excluded.file_size,
                    last_access = datetime('now')",
                params![drive_id, item_id, file_size],
            );
        }

        self.evict_if_needed().await?;

        Ok(())
    }

    pub async fn remove(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
        let path = self.content_path(drive_id, item_id);
        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("remove cache failed: {e}")))?;
        }
        if let Ok(conn) = self.tracker.lock() {
            let _ = conn.execute(
                "DELETE FROM cache_entries WHERE drive_id = ?1 AND item_id = ?2",
                params![drive_id, item_id],
            );
        }
        Ok(())
    }

    pub async fn clear(&self) -> cloudmount_core::Result<()> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir)
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("clear cache failed: {e}")))?;
            fs::create_dir_all(&self.base_dir).await.map_err(|e| {
                cloudmount_core::Error::Cache(format!("recreate cache dir failed: {e}"))
            })?;
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

    async fn evict_if_needed(&self) -> cloudmount_core::Result<()> {
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
            let conn = self
                .tracker
                .lock()
                .map_err(|e| cloudmount_core::Error::Cache(format!("tracker lock failed: {e}")))?;
            let mut stmt = conn
                .prepare(
                    "SELECT drive_id, item_id, file_size FROM cache_entries ORDER BY last_access ASC",
                )
                .map_err(|e| {
                    cloudmount_core::Error::Cache(format!("prepare eviction query failed: {e}"))
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
                    cloudmount_core::Error::Cache(format!("eviction query failed: {e}"))
                })?;

            let entries: Vec<_> = rows.flatten().collect();
            entries
        };

        let mut freed: u64 = 0;
        for (drive_id, item_id, size) in entries {
            if freed >= to_free {
                break;
            }
            let path = self.content_path(&drive_id, &item_id);
            if path.exists() {
                let _ = fs::remove_file(&path).await;
            }
            if let Ok(conn) = self.tracker.lock() {
                let _ = conn.execute(
                    "DELETE FROM cache_entries WHERE drive_id = ?1 AND item_id = ?2",
                    params![drive_id, item_id],
                );
            }
            freed += size as u64;
            tracing::debug!("evicted cache entry {drive_id}/{item_id} ({size} bytes)");
        }

        tracing::info!("cache eviction freed {freed} bytes (target was {to_free})");
        Ok(())
    }
}
