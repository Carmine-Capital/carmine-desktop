use std::sync::Mutex;

use rusqlite::{Connection, params};

use carminedesktop_core::types::DriveItem;

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: &std::path::Path) -> carminedesktop_core::Result<Self> {
        let conn = Connection::open(path).map_err(|e| {
            carminedesktop_core::Error::Cache(format!("failed to open SQLite: {e}"))
        })?;

        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| carminedesktop_core::Error::Cache(format!("failed to set busy_timeout: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("failed to set pragmas: {e}")))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_tables()?;
        Ok(store)
    }

    fn create_tables(&self) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS items (
                inode INTEGER PRIMARY KEY,
                item_id TEXT NOT NULL UNIQUE,
                parent_inode INTEGER,
                drive_id TEXT NOT NULL,
                name TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                is_folder INTEGER NOT NULL DEFAULT 0,
                etag TEXT,
                mtime TEXT,
                ctime TEXT,
                json_data TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_items_parent ON items(parent_inode);
            CREATE INDEX IF NOT EXISTS idx_items_item_id ON items(item_id);

            CREATE TABLE IF NOT EXISTS delta_tokens (
                drive_id TEXT PRIMARY KEY,
                token TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS sync_state (
                item_id TEXT PRIMARY KEY,
                local_etag TEXT,
                remote_etag TEXT,
                pending_upload INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS pinned_folders (
                drive_id   TEXT NOT NULL,
                item_id    TEXT NOT NULL,
                pinned_at  TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT NOT NULL,
                PRIMARY KEY (drive_id, item_id)
            );

",
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("failed to create tables: {e}")))?;

        Ok(())
    }

    pub fn upsert_item(
        &self,
        inode: u64,
        drive_id: &str,
        item: &DriveItem,
        parent_inode: Option<u64>,
    ) -> carminedesktop_core::Result<()> {
        let json = serde_json::to_string(item)
            .map_err(|e| carminedesktop_core::Error::Cache(format!("serialize failed: {e}")))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO items (inode, item_id, parent_inode, drive_id, name, size, is_folder, etag, mtime, ctime, json_data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(item_id) DO UPDATE SET
                parent_inode = excluded.parent_inode,
                name = excluded.name,
                size = excluded.size,
                is_folder = excluded.is_folder,
                etag = excluded.etag,
                mtime = excluded.mtime,
                ctime = excluded.ctime,
                json_data = excluded.json_data",
            params![
                inode as i64,
                item.id,
                parent_inode.map(|i| i as i64),
                drive_id,
                item.name,
                item.size,
                item.is_folder() as i32,
                item.etag,
                item.last_modified.map(|d| d.to_rfc3339()),
                item.created.map(|d| d.to_rfc3339()),
                json,
            ],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("upsert failed: {e}")))?;

        Ok(())
    }

    /// Return the stored inode for an item_id without deserializing JSON.
    pub fn get_inode(&self, item_id: &str) -> carminedesktop_core::Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .query_row(
                "SELECT inode FROM items WHERE item_id = ?1",
                params![item_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;
        Ok(result.map(|i| i as u64))
    }

    pub fn get_item_by_id(
        &self,
        item_id: &str,
    ) -> carminedesktop_core::Result<Option<(u64, DriveItem)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare_cached("SELECT inode, json_data FROM items WHERE item_id = ?1")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;

        let result = stmt
            .query_row(params![item_id], |row| {
                let inode: i64 = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((inode as u64, json))
            })
            .optional()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;

        match result {
            Some((inode, json)) => {
                let item: DriveItem = serde_json::from_str(&json).map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("deserialize failed: {e}"))
                })?;
                Ok(Some((inode, item)))
            }
            None => Ok(None),
        }
    }

    pub fn get_item_by_inode(&self, inode: u64) -> carminedesktop_core::Result<Option<DriveItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare_cached("SELECT json_data FROM items WHERE inode = ?1")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;

        let result = stmt
            .query_row(params![inode as i64], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .optional()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;

        match result {
            Some(json) => {
                let item: DriveItem = serde_json::from_str(&json).map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("deserialize failed: {e}"))
                })?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }

    pub fn get_children(
        &self,
        parent_inode: u64,
    ) -> carminedesktop_core::Result<Vec<(u64, DriveItem)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare_cached("SELECT inode, json_data FROM items WHERE parent_inode = ?1")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![parent_inode as i64], |row| {
                let inode: i64 = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((inode as u64, json))
            })
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;

        let mut children = Vec::new();
        for row in rows {
            let (inode, json) = row
                .map_err(|e| carminedesktop_core::Error::Cache(format!("row read failed: {e}")))?;
            let item: DriveItem = serde_json::from_str(&json).map_err(|e| {
                carminedesktop_core::Error::Cache(format!("deserialize failed: {e}"))
            })?;
            children.push((inode, item));
        }

        Ok(children)
    }

    pub fn delete_children(&self, parent_inode: u64) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM items WHERE parent_inode = ?1",
            params![parent_inode as i64],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("delete children failed: {e}")))?;
        Ok(())
    }

    pub fn delete_item(&self, item_id: &str) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM items WHERE item_id = ?1", params![item_id])
            .map_err(|e| carminedesktop_core::Error::Cache(format!("delete failed: {e}")))?;
        Ok(())
    }

    pub fn get_delta_token(&self, drive_id: &str) -> carminedesktop_core::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare_cached("SELECT token FROM delta_tokens WHERE drive_id = ?1")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;

        stmt.query_row(params![drive_id], |row| row.get(0))
            .optional()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))
    }

    pub fn set_delta_token(&self, drive_id: &str, token: &str) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO delta_tokens (drive_id, token) VALUES (?1, ?2)
             ON CONFLICT(drive_id) DO UPDATE SET token = excluded.token, updated_at = datetime('now')",
            params![drive_id, token],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("set delta token failed: {e}")))?;
        Ok(())
    }

    pub fn max_inode(&self) -> carminedesktop_core::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let max: Option<i64> = conn
            .query_row("SELECT MAX(inode) FROM items", [], |row| row.get(0))
            .map_err(|e| {
                carminedesktop_core::Error::Cache(format!("max inode query failed: {e}"))
            })?;
        Ok(max.unwrap_or(0) as u64)
    }

    /// Return all (inode, item_id) pairs for seeding the InodeTable at mount
    /// startup.  This ensures VFS and SQLite agree on inode values for items
    /// persisted by offline download.
    pub fn all_inode_pairs(&self) -> carminedesktop_core::Result<Vec<(u64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT inode, item_id FROM items")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                let inode: i64 = row.get(0)?;
                let item_id: String = row.get(1)?;
                Ok((inode as u64, item_id))
            })
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;
        let mut pairs = Vec::new();
        for row in rows {
            pairs.push(
                row.map_err(|e| carminedesktop_core::Error::Cache(format!("row failed: {e}")))?,
            );
        }
        Ok(pairs)
    }

    pub fn clear(&self) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM items; DELETE FROM delta_tokens; DELETE FROM sync_state;")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("clear failed: {e}")))?;
        Ok(())
    }

    pub fn apply_delta(
        &self,
        drive_id: &str,
        items: &[(u64, DriveItem, Option<u64>)],
        deleted_ids: &[String],
        new_delta_token: &str,
    ) -> carminedesktop_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("transaction failed: {e}")))?;

        for (inode, item, parent_inode) in items {
            let json = serde_json::to_string(item)
                .map_err(|e| carminedesktop_core::Error::Cache(format!("serialize failed: {e}")))?;

            tx.execute(
                "INSERT INTO items (inode, item_id, parent_inode, drive_id, name, size, is_folder, etag, mtime, ctime, json_data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(item_id) DO UPDATE SET
                    parent_inode = excluded.parent_inode,
                    name = excluded.name,
                    size = excluded.size,
                    is_folder = excluded.is_folder,
                    etag = excluded.etag,
                    mtime = excluded.mtime,
                    ctime = excluded.ctime,
                    json_data = excluded.json_data",
                params![
                    *inode as i64,
                    item.id,
                    parent_inode.map(|i| i as i64),
                    drive_id,
                    item.name,
                    item.size,
                    item.is_folder() as i32,
                    item.etag,
                    item.last_modified.map(|d| d.to_rfc3339()),
                    item.created.map(|d| d.to_rfc3339()),
                    json,
                ],
            )
            .map_err(|e| carminedesktop_core::Error::Cache(format!("upsert in delta failed: {e}")))?;
        }

        for id in deleted_ids {
            tx.execute("DELETE FROM items WHERE item_id = ?1", params![id])
                .map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("delete in delta failed: {e}"))
                })?;
        }

        tx.execute(
            "INSERT INTO delta_tokens (drive_id, token) VALUES (?1, ?2)
             ON CONFLICT(drive_id) DO UPDATE SET token = excluded.token, updated_at = datetime('now')",
            params![drive_id, new_delta_token],
        )
        .map_err(|e| carminedesktop_core::Error::Cache(format!("set delta token failed: {e}")))?;

        tx.commit()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("commit failed: {e}")))?;

        Ok(())
    }
}

trait OptionalExt<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
