use std::sync::Mutex;

use rusqlite::{Connection, params};

/// A single pinned folder record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PinnedFolder {
    pub drive_id: String,
    pub item_id: String,
    pub pinned_at: String,
    pub expires_at: String,
}

/// Persistent store for offline-pinned folder records.
///
/// Opens a **separate** `Connection` to the same WAL-mode database used by
/// `SqliteStore` and `DiskCache`.  A dedicated connection avoids contention
/// with those hot-path mutexes during eviction-filter checks.
pub struct PinStore {
    conn: Mutex<Connection>,
}

impl PinStore {
    /// Open a PinStore on the same database file as SqliteStore.
    ///
    /// The `pinned_folders` table must already exist (created by
    /// `SqliteStore::create_tables()`).  This method only opens a second
    /// WAL-mode connection — it does **not** create the table.
    pub fn open(db_path: &std::path::Path) -> carminedesktop_core::Result<Self> {
        let conn = Connection::open(db_path).map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store: failed to open db: {e}"))
        })?;
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!(
                    "pin store: failed to set busy_timeout: {e}"
                ))
            })?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store: failed to set pragmas: {e}"))
            })?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert or refresh a pin.  Upserts: if already pinned, updates
    /// `pinned_at` and `expires_at`.
    pub fn pin(
        &self,
        drive_id: &str,
        item_id: &str,
        ttl_secs: u64,
    ) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
        })?;
        conn.execute(
            "INSERT INTO pinned_folders (drive_id, item_id, pinned_at, expires_at)
             VALUES (?1, ?2, datetime('now'), datetime('now', '+' || ?3 || ' seconds'))
             ON CONFLICT(drive_id, item_id) DO UPDATE SET
                pinned_at = datetime('now'),
                expires_at = datetime('now', '+' || ?3 || ' seconds')",
            params![drive_id, item_id, ttl_secs as i64],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("pin store insert failed: {e}")))?;
        Ok(())
    }

    /// Remove a pin record.  No-op if the folder is not pinned.
    pub fn unpin(&self, drive_id: &str, item_id: &str) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
        })?;
        conn.execute(
            "DELETE FROM pinned_folders WHERE drive_id = ?1 AND item_id = ?2",
            params![drive_id, item_id],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("pin store delete failed: {e}")))?;
        Ok(())
    }

    /// Check if a specific folder is pinned (non-expired).
    pub fn is_pinned(&self, drive_id: &str, item_id: &str) -> bool {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM pinned_folders
             WHERE drive_id = ?1 AND item_id = ?2 AND expires_at > datetime('now')",
            params![drive_id, item_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    /// Return all expired pin records.
    pub fn list_expired(&self) -> carminedesktop_core::Result<Vec<PinnedFolder>> {
        let conn = self.conn.lock().map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
        })?;
        let mut stmt = conn
            .prepare(
                "SELECT drive_id, item_id, pinned_at, expires_at
                 FROM pinned_folders WHERE expires_at <= datetime('now')",
            )
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store prepare failed: {e}"))
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PinnedFolder {
                    drive_id: row.get(0)?,
                    item_id: row.get(1)?,
                    pinned_at: row.get(2)?,
                    expires_at: row.get(3)?,
                })
            })
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store query failed: {e}"))
            })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store row read failed: {e}"))
            })?);
        }
        Ok(result)
    }

    /// Return all pin records (for eviction filter and UI).
    pub fn list_all(&self) -> carminedesktop_core::Result<Vec<PinnedFolder>> {
        let conn = self.conn.lock().map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
        })?;
        let mut stmt = conn
            .prepare("SELECT drive_id, item_id, pinned_at, expires_at FROM pinned_folders")
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store prepare failed: {e}"))
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PinnedFolder {
                    drive_id: row.get(0)?,
                    item_id: row.get(1)?,
                    pinned_at: row.get(2)?,
                    expires_at: row.get(3)?,
                })
            })
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store query failed: {e}"))
            })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store row read failed: {e}"))
            })?);
        }
        Ok(result)
    }

    /// Compute on-demand health status for all non-expired pins.
    ///
    /// For each pinned folder:
    /// - Count total files in the pinned subtree (files have `is_folder = 0`)
    /// - Count how many of those files have an entry in `cache_entries`
    ///
    /// The `stale_pins` set contains `(drive_id, item_id)` pairs that the
    /// caller has determined are stale (server content changed since last pin
    /// sync). This method does NOT compute staleness itself.
    ///
    /// Returns `Vec` of `(PinnedFolder, total_files, cached_files)`.
    pub fn health(
        &self,
        stale_pins: &std::collections::HashSet<(String, String)>,
    ) -> carminedesktop_core::Result<Vec<(PinnedFolder, usize, usize)>> {
        let _ = stale_pins; // reserved for future use — staleness set by caller

        let conn = self.conn.lock().map_err(|e| {
            carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
        })?;

        // Get all non-expired pins
        let mut pin_stmt = conn
            .prepare(
                "SELECT drive_id, item_id, pinned_at, expires_at
                 FROM pinned_folders
                 WHERE expires_at > datetime('now')",
            )
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("pin store prepare failed: {e}"))
            })?;

        let pins: Vec<PinnedFolder> = pin_stmt
            .query_map([], |row| {
                Ok(PinnedFolder {
                    drive_id: row.get(0)?,
                    item_id: row.get(1)?,
                    pinned_at: row.get(2)?,
                    expires_at: row.get(3)?,
                })
            })
            .map_err(|e| carminedesktop_core::Error::Cache(format!("pin query failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        let mut results = Vec::with_capacity(pins.len());

        for pin in pins {
            // Count total files in pinned subtree using a recursive CTE.
            // Files are items where is_folder = 0.
            let total_files: usize = conn
                .query_row(
                    "WITH RECURSIVE subtree(inode) AS (
                        SELECT inode FROM items WHERE item_id = ?1
                        UNION ALL
                        SELECT i.inode FROM items i
                        JOIN subtree s ON i.parent_inode = s.inode
                    )
                    SELECT COUNT(*) FROM items
                    WHERE inode IN (SELECT inode FROM subtree)
                    AND is_folder = 0
                    AND item_id != ?1",
                    params![pin.item_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as usize;

            // Count cached files: intersect subtree files with cache_entries.
            let cached_files: usize = conn
                .query_row(
                    "WITH RECURSIVE subtree(inode) AS (
                        SELECT inode FROM items WHERE item_id = ?1
                        UNION ALL
                        SELECT i.inode FROM items i
                        JOIN subtree s ON i.parent_inode = s.inode
                    )
                    SELECT COUNT(*) FROM items
                    WHERE inode IN (SELECT inode FROM subtree)
                    AND is_folder = 0
                    AND item_id != ?1
                    AND EXISTS (
                        SELECT 1 FROM cache_entries ce
                        WHERE ce.item_id = items.item_id AND ce.drive_id = ?2
                    )",
                    params![pin.item_id, pin.drive_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as usize;

            results.push((pin, total_files, cached_files));
        }

        Ok(results)
    }

    /// Check if an item is protected by any pin — either the item itself is
    /// pinned, or one of its ancestors (via the `items` table parent chain) is
    /// pinned.  Used by the disk-cache eviction filter.
    pub fn is_protected(&self, drive_id: &str, item_id: &str) -> bool {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Fast path: is this item directly pinned?
        let direct: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pinned_folders
                 WHERE drive_id = ?1 AND item_id = ?2 AND expires_at > datetime('now')",
                params![drive_id, item_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if direct {
            return true;
        }

        // Walk parent chain via the items table.
        let mut current_item_id = item_id.to_string();
        for _ in 0..50 {
            let parent_item_id: Option<String> = conn
                .query_row(
                    "SELECT p.item_id FROM items c
                     JOIN items p ON p.inode = c.parent_inode
                     WHERE c.item_id = ?1",
                    params![current_item_id],
                    |row| row.get(0),
                )
                .ok();

            match parent_item_id {
                Some(pid) => {
                    let pinned: bool = conn
                        .query_row(
                            "SELECT COUNT(*) FROM pinned_folders
                             WHERE drive_id = ?1 AND item_id = ?2 AND expires_at > datetime('now')",
                            params![drive_id, pid],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap_or(0)
                        > 0;

                    if pinned {
                        return true;
                    }
                    current_item_id = pid;
                }
                None => break,
            }
        }

        false
    }
}
