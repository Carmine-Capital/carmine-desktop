# Design: Make Available Offline

## Overview

The offline feature adds a pin/unpin lifecycle for folders on VFS mounts. A `PinStore` (SQLite table in the existing per-mount DB) tracks pinned folders. An `OfflineManager` facade orchestrates size validation, recursive downloads, TTL expiry, and delta sync re-downloads. On Windows, Explorer context menu verbs invoke the running app via CLI args forwarded through the single-instance plugin, with a named pipe fallback. Eviction protection is injected into `DiskCache` as a callback predicate.

## Components

### PinStore

- **Responsibility**: CRUD operations on the `pinned_folders` SQLite table. Pure data access — no business logic.
- **File(s)**: `crates/carminedesktop-cache/src/pin_store.rs` (NEW)
- **Implements**: CAP-01

#### Data Model

The table is created in `SqliteStore::create_tables()` (`crates/carminedesktop-cache/src/sqlite.rs`, line 29), appended to the existing `CREATE TABLE IF NOT EXISTS` batch:

```sql
CREATE TABLE IF NOT EXISTS pinned_folders (
    drive_id   TEXT NOT NULL,
    item_id    TEXT NOT NULL,
    pinned_at  TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    PRIMARY KEY (drive_id, item_id)
);
```

#### Struct and Interface

```rust
// crates/carminedesktop-cache/src/pin_store.rs

use std::sync::Mutex;
use rusqlite::{Connection, params};

pub struct PinStore {
    conn: Mutex<Connection>,
}

/// A single pinned folder record.
#[derive(Debug, Clone)]
pub struct PinnedFolder {
    pub drive_id: String,
    pub item_id: String,
    pub pinned_at: String,
    pub expires_at: String,
}

impl PinStore {
    /// Open a PinStore sharing the same database file as the SqliteStore.
    /// Opens a SECOND connection to the same WAL-mode DB (safe for concurrent readers).
    pub fn open(db_path: &std::path::Path) -> carminedesktop_core::Result<Self> { ... }

    /// Insert or refresh a pin. Upserts: if already pinned, updates timestamps.
    pub fn pin(
        &self,
        drive_id: &str,
        item_id: &str,
        ttl_secs: u64,
    ) -> carminedesktop_core::Result<()> { ... }

    /// Remove a pin record. No-op if not pinned.
    pub fn unpin(&self, drive_id: &str, item_id: &str) -> carminedesktop_core::Result<()> { ... }

    /// Check if a specific folder is pinned (non-expired).
    pub fn is_pinned(&self, drive_id: &str, item_id: &str) -> bool { ... }

    /// Return all expired pin records.
    pub fn list_expired(&self) -> carminedesktop_core::Result<Vec<PinnedFolder>> { ... }

    /// Return all pin records (for eviction filter and UI).
    pub fn list_all(&self) -> carminedesktop_core::Result<Vec<PinnedFolder>> { ... }
}
```

#### Connection Strategy

`PinStore` opens its own `Connection` to the same DB file (WAL mode supports concurrent readers). This avoids contending with `SqliteStore`'s `Mutex<Connection>` during eviction filter checks, which happen on the hot path. The connection uses the same pragmas: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`.

The `pinned_folders` table DDL is added to `SqliteStore::create_tables()` so it exists regardless of whether `PinStore` is instantiated (forward compatibility). `PinStore::open()` does NOT create the table — it relies on `SqliteStore` having done so.

#### Pin SQL

```sql
-- pin (upsert)
INSERT INTO pinned_folders (drive_id, item_id, pinned_at, expires_at)
VALUES (?1, ?2, datetime('now'), datetime('now', '+' || ?3 || ' seconds'))
ON CONFLICT(drive_id, item_id) DO UPDATE SET
    pinned_at = datetime('now'),
    expires_at = datetime('now', '+' || excluded.expires_at || ' seconds');
```

Note: The upsert for `expires_at` needs the raw TTL seconds, not the computed datetime. Implementation will compute `expires_at` in Rust and pass it as a string:

```rust
pub fn pin(&self, drive_id: &str, item_id: &str, ttl_secs: u64) -> carminedesktop_core::Result<()> {
    let conn = self.conn.lock().map_err(|e|
        carminedesktop_core::Error::Cache(format!("pin store lock failed: {e}"))
    )?;
    conn.execute(
        "INSERT INTO pinned_folders (drive_id, item_id, pinned_at, expires_at)
         VALUES (?1, ?2, datetime('now'), datetime('now', '+' || ?3 || ' seconds'))
         ON CONFLICT(drive_id, item_id) DO UPDATE SET
            pinned_at = datetime('now'),
            expires_at = datetime('now', '+' || ?3 || ' seconds')",
        params![drive_id, item_id, ttl_secs as i64],
    ).map_err(|e|
        carminedesktop_core::Error::Cache(format!("pin store insert failed: {e}"))
    )?;
    Ok(())
}
```

#### Test Strategy

- **File**: `crates/carminedesktop-cache/tests/test_pin_store.rs` (NEW)
- Tests use a temp SQLite DB (`std::env::temp_dir()`)
- `test_pin_store_pin_and_is_pinned()` — pin a folder, verify `is_pinned()` returns true
- `test_pin_store_unpin()` — pin then unpin, verify `is_pinned()` returns false
- `test_pin_store_upsert_refreshes_timestamps()` — pin twice, verify `pinned_at` is updated
- `test_pin_store_list_expired()` — pin with TTL=0, verify it appears in `list_expired()` (use `tokio::time::pause()` or insert with past `expires_at`)
- `test_pin_store_list_all()` — pin multiple folders, verify all returned

---

### DiskCache Eviction Filter

- **Responsibility**: Skip pinned items during LRU eviction.
- **File(s)**: `crates/carminedesktop-cache/src/disk.rs` (MODIFY)
- **Implements**: CAP-02

#### Changes to `DiskCache` struct (line 9)

Add an optional eviction filter field:

```rust
pub struct DiskCache {
    base_dir: PathBuf,
    max_size_bytes: AtomicU64,
    tracker: Mutex<Connection>,
    eviction_filter: std::sync::RwLock<Option<Arc<dyn Fn(&str, &str) -> bool + Send + Sync>>>,
}
```

Using `RwLock` because the filter is set once and read many times during eviction. The `Arc<dyn Fn>` allows the closure to capture a `PinStore` reference.

#### New method on `DiskCache`

```rust
/// Set a filter predicate for eviction. If the filter returns `true` for a
/// (drive_id, item_id) pair, that entry is skipped during LRU eviction.
pub fn set_eviction_filter(&self, filter: Arc<dyn Fn(&str, &str) -> bool + Send + Sync>) {
    *self.eviction_filter.write().unwrap() = Some(filter);
}
```

#### Changes to `DiskCache::new()` (line 16)

Initialize the new field:

```rust
Ok(Self {
    base_dir,
    max_size_bytes: AtomicU64::new(max_size_bytes),
    tracker: Mutex::new(conn),
    eviction_filter: std::sync::RwLock::new(None),
})
```

#### Changes to `evict_if_needed()` (line 254, specifically the loop at line 295)

Current code (lines 294–308):
```rust
let mut freed: u64 = 0;
for (drive_id, item_id, size) in entries {
    if freed >= to_free {
        break;
    }
    let path = self.content_path(&drive_id, &item_id);
    let _ = fs::remove_file(&path).await;
    // ...
    freed += size as u64;
}
```

Modified code:
```rust
let filter = self.eviction_filter.read().unwrap().clone();
let mut freed: u64 = 0;
for (drive_id, item_id, size) in entries {
    if freed >= to_free {
        break;
    }
    // Skip protected (pinned) entries
    if let Some(ref f) = filter {
        if f(&drive_id, &item_id) {
            continue;
        }
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
        "cache eviction could not free enough space: freed {freed} bytes, target was {to_free} \
         (some entries are protected by offline pins)"
    );
}
```

#### Test Strategy

- **File**: `crates/carminedesktop-cache/tests/test_disk_eviction.rs` (NEW)
- `test_disk_eviction_skips_protected_entries()` — set filter that protects one entry, fill cache past max, verify protected entry survives eviction
- `test_disk_eviction_no_filter()` — verify default behavior unchanged when no filter is set
- `test_disk_eviction_all_protected()` — all entries protected, verify eviction stops gracefully with warning

---

### OfflineManager

- **Responsibility**: Facade orchestrating the full pin lifecycle: size validation, pin record creation, background recursive download, TTL expiry processing, and delta sync re-download coordination.
- **File(s)**: `crates/carminedesktop-cache/src/offline.rs` (NEW)
- **Implements**: CAP-03, CAP-08, CAP-10

#### Struct

```rust
// crates/carminedesktop-cache/src/offline.rs

use std::sync::Arc;
use carminedesktop_core::types::DriveItem;
use carminedesktop_graph::GraphClient;
use crate::pin_store::PinStore;
use crate::disk::DiskCache;

pub struct OfflineManager {
    pin_store: Arc<PinStore>,
    graph: Arc<GraphClient>,
    disk: Arc<DiskCache>,
    drive_id: String,
    ttl_secs: std::sync::atomic::AtomicU64,
    max_folder_bytes: std::sync::atomic::AtomicU64,
}

/// Result of a pin attempt.
pub enum PinResult {
    /// Pin succeeded, background download spawned.
    Ok,
    /// Pin rejected (e.g. folder too large).
    Rejected { reason: String },
}

impl OfflineManager {
    pub fn new(
        pin_store: Arc<PinStore>,
        graph: Arc<GraphClient>,
        disk: Arc<DiskCache>,
        drive_id: String,
        ttl_secs: u64,
        max_folder_bytes: u64,
    ) -> Self { ... }

    /// Validate folder size and create a pin record. Spawns a background
    /// recursive download task. Returns immediately.
    pub async fn pin_folder(
        &self,
        item_id: &str,
        folder_name: &str,
    ) -> carminedesktop_core::Result<PinResult> { ... }

    /// Remove a pin record. Cached files become eligible for LRU eviction.
    pub fn unpin_folder(
        &self,
        item_id: &str,
    ) -> carminedesktop_core::Result<()> { ... }

    /// Process expired pins: remove records, log each removal.
    /// Called from the delta sync loop.
    pub fn process_expired(&self) -> carminedesktop_core::Result<Vec<String>> { ... }

    /// Re-download files that changed (per delta sync) and belong to pinned folders.
    pub async fn redownload_changed_items(
        &self,
        changed_items: &[DriveItem],
    ) -> carminedesktop_core::Result<()> { ... }

    /// Update TTL for future pins (does not affect existing pins).
    pub fn set_ttl_secs(&self, ttl: u64) { ... }

    /// Update max folder size for future pins.
    pub fn set_max_folder_bytes(&self, max: u64) { ... }
}
```

#### `pin_folder()` Implementation Detail

```rust
pub async fn pin_folder(
    &self,
    item_id: &str,
    folder_name: &str,
) -> carminedesktop_core::Result<PinResult> {
    // 1. Fetch item metadata to validate it's a folder and check size
    let item = self.graph.get_item(&self.drive_id, item_id).await?;

    if !item.is_folder() {
        return Ok(PinResult::Rejected {
            reason: "only folders can be pinned for offline use".to_string(),
        });
    }

    // 2. Size validation
    let folder_size = if item.size <= 0 {
        // Graph API sometimes returns 0 for folder size; re-fetch
        let refetched = self.graph.get_item(&self.drive_id, item_id).await?;
        refetched.size.max(0) as u64
    } else {
        item.size as u64
    };

    let max = self.max_folder_bytes.load(std::sync::atomic::Ordering::Relaxed);
    if max > 0 && folder_size > max {
        let human_size = format_bytes(folder_size);
        let human_max = format_bytes(max);
        return Ok(PinResult::Rejected {
            reason: format!("{human_size} exceeds the {human_max} limit"),
        });
    }

    // 3. Create/refresh pin record
    let ttl = self.ttl_secs.load(std::sync::atomic::Ordering::Relaxed);
    self.pin_store.pin(&self.drive_id, item_id, ttl)?;

    // 4. Spawn background recursive download
    let graph = self.graph.clone();
    let disk = self.disk.clone();
    let drive_id = self.drive_id.clone();
    let item_id = item_id.to_string();
    tokio::spawn(async move {
        if let Err(e) = recursive_download(&graph, &disk, &drive_id, &item_id).await {
            tracing::error!("offline download failed for {item_id}: {e}");
        }
    });

    Ok(PinResult::Ok)
}
```

#### `recursive_download()` Helper

```rust
/// Recursively download all file descendants of a folder.
async fn recursive_download(
    graph: &GraphClient,
    disk: &DiskCache,
    drive_id: &str,
    folder_id: &str,
) -> carminedesktop_core::Result<()> {
    let children = graph.list_children(drive_id, folder_id).await?;

    for child in &children {
        if child.is_folder() {
            // Recurse into subfolders
            Box::pin(recursive_download(graph, disk, drive_id, &child.id)).await?;
        } else {
            // Download file content if not already cached
            if disk.get(drive_id, &child.id).await.is_none() {
                let content = graph.download_content(drive_id, &child.id).await?;
                disk.put(drive_id, &child.id, &content, child.etag.as_deref()).await?;
                tracing::debug!("offline: downloaded {}/{}", drive_id, child.id);
            }
        }
    }

    Ok(())
}
```

#### `redownload_changed_items()` Implementation Detail

```rust
pub async fn redownload_changed_items(
    &self,
    changed_items: &[DriveItem],
) -> carminedesktop_core::Result<()> {
    if changed_items.is_empty() {
        return Ok(());
    }

    let pinned = self.pin_store.list_all()?;
    if pinned.is_empty() {
        return Ok(());
    }

    let pinned_ids: std::collections::HashSet<&str> =
        pinned.iter().map(|p| p.item_id.as_str()).collect();

    for item in changed_items {
        // Check if this item's parent chain includes a pinned folder.
        // Simple heuristic: check if any pinned folder ID appears as a
        // parent_reference.id in the item's ancestry.
        let parent_id = item
            .parent_reference
            .as_ref()
            .and_then(|pr| pr.id.as_deref());

        if let Some(pid) = parent_id {
            if pinned_ids.contains(pid) {
                // Re-download this file
                let content = self.graph.download_content(&self.drive_id, &item.id).await?;
                self.disk.put(&self.drive_id, &item.id, &content, item.etag.as_deref()).await?;
                tracing::debug!("offline: re-downloaded changed item {}", item.id);
            }
        }
    }

    Ok(())
}
```

Note: This checks only the immediate parent. For deeply nested pinned folders, the initial recursive download ensures all descendants are cached. Delta sync `changed_items` only contains items whose eTag changed — their immediate parent is sufficient to determine if they're in a pinned subtree, because the pin is on the folder and all its descendants were downloaded. If a file is moved into a pinned folder, delta sync will report it with the new parent, and the check will match.

#### `process_expired()` Implementation Detail

```rust
pub fn process_expired(&self) -> carminedesktop_core::Result<Vec<String>> {
    let expired = self.pin_store.list_expired()?;
    let mut expired_names = Vec::new();
    for record in &expired {
        self.pin_store.unpin(&record.drive_id, &record.item_id)?;
        tracing::info!(
            "offline pin expired for {}/{}",
            record.drive_id, record.item_id
        );
        expired_names.push(record.item_id.clone());
    }
    Ok(expired_names)
}
```

#### Eviction Filter Wiring

The eviction filter closure is created in `CacheManager::new()` and set on `DiskCache`:

```rust
// In CacheManager::new(), after creating disk and pin_store:
let pin_store_for_filter = pin_store.clone();
let drive_id_for_filter = drive_id.clone(); // Need drive_id context
disk.set_eviction_filter(Arc::new(move |_drive_id: &str, item_id: &str| {
    // Check if this item's parent is a pinned folder.
    // For simplicity, check all pinned folder IDs against the item's
    // cache entry. The PinStore query is fast (indexed primary key).
    pin_store_for_filter.is_pinned(_drive_id, item_id)
}));
```

However, this only protects items whose `item_id` is directly a pinned folder. Files *inside* pinned folders need protection too. The filter needs to check if the item is a descendant of any pinned folder. Since `cache_entries` doesn't store parent relationships, we use a broader approach:

The `PinStore` maintains a helper method that checks the `items` table (SQLite metadata) to walk the parent chain:

```rust
/// Check if an item is protected by any pin (is the item itself pinned,
/// or is any of its ancestors pinned).
pub fn is_protected(&self, drive_id: &str, item_id: &str) -> bool {
    let conn = match self.conn.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Fast path: is this item directly pinned?
    let direct: bool = conn.query_row(
        "SELECT COUNT(*) FROM pinned_folders
         WHERE drive_id = ?1 AND item_id = ?2 AND expires_at > datetime('now')",
        params![drive_id, item_id],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if direct {
        return true;
    }

    // Walk parent chain via the items table (SQLite metadata store).
    // items table has: inode, item_id, parent_inode, ...
    // We look up this item's parent_inode, then that parent's item_id, etc.
    let mut current_item_id = item_id.to_string();
    for _ in 0..50 {  // depth limit to prevent infinite loops
        let parent_item_id: Option<String> = conn.query_row(
            "SELECT p.item_id FROM items c
             JOIN items p ON p.inode = c.parent_inode
             WHERE c.item_id = ?1",
            params![current_item_id],
            |row| row.get(0),
        ).ok();

        match parent_item_id {
            Some(pid) => {
                let pinned: bool = conn.query_row(
                    "SELECT COUNT(*) FROM pinned_folders
                     WHERE drive_id = ?1 AND item_id = ?2 AND expires_at > datetime('now')",
                    params![drive_id, pid],
                    |row| row.get::<_, i64>(0),
                ).unwrap_or(0) > 0;

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
```

The eviction filter then uses `is_protected()`:

```rust
disk.set_eviction_filter(Arc::new(move |drive_id: &str, item_id: &str| {
    pin_store_for_filter.is_protected(drive_id, item_id)
}));
```

#### Construction and Ownership

`OfflineManager` is created per-mount in `start_mount_common()` (`main.rs`, line 936). It needs `Arc<GraphClient>`, `Arc<DiskCache>` (from `CacheManager`), and `Arc<PinStore>`.

**Changes to `CacheManager`** (`crates/carminedesktop-cache/src/manager.rs`):

```rust
pub struct CacheManager {
    pub memory: MemoryCache,
    pub sqlite: SqliteStore,
    pub disk: DiskCache,
    pub writeback: WriteBackBuffer,
    pub dirty_inodes: DashSet<u64>,
    pub pin_store: Arc<PinStore>,  // NEW
}
```

`CacheManager::new()` creates the `PinStore` and wires the eviction filter:

```rust
pub fn new(
    cache_dir: PathBuf,
    db_path: PathBuf,
    max_cache_bytes: u64,
    ttl_secs: Option<u64>,
) -> carminedesktop_core::Result<Self> {
    std::fs::create_dir_all(&cache_dir).map_err(|e| {
        carminedesktop_core::Error::Cache(format!("create cache dir failed: {e}"))
    })?;

    let sqlite = SqliteStore::open(&db_path)?;
    let memory = MemoryCache::new(ttl_secs);
    let disk = DiskCache::new(cache_dir.join("content"), max_cache_bytes, &db_path)?;
    let writeback = WriteBackBuffer::new(cache_dir);
    let pin_store = Arc::new(PinStore::open(&db_path)?);

    // Wire eviction protection
    let ps = pin_store.clone();
    disk.set_eviction_filter(Arc::new(move |drive_id: &str, item_id: &str| {
        ps.is_protected(drive_id, item_id)
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
```

**`OfflineManager` is created in `start_mount_common()`** and stored alongside the mount cache. Changes to `MountCacheEntry` type alias (`main.rs`, line 86):

```rust
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
    Arc<OfflineManager>,  // NEW
);
```

In `start_mount_common()`, after creating the `CacheManager` (line 1031):

```rust
let (offline_ttl, offline_max_bytes) = {
    let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
    (
        cfg.offline_ttl_secs,
        parse_cache_size(&cfg.offline_max_folder_size),
    )
};

let offline_manager = Arc::new(carminedesktop_cache::offline::OfflineManager::new(
    cache.pin_store.clone(),
    state.graph.clone(),
    Arc::new(cache.disk.clone()),  // Note: DiskCache needs to be Arc-wrapped or accessed via CacheManager
    drive_id.to_string(),
    offline_ttl,
    offline_max_bytes,
));
```

**Important**: `DiskCache` is currently owned by `CacheManager` (not `Arc`). For `OfflineManager` to hold a reference, we have two options:
1. Change `CacheManager.disk` to `Arc<DiskCache>` — cleanest but touches all call sites.
2. Have `OfflineManager` take `Arc<CacheManager>` and access `disk` through it.

**Decision**: Option 2 — `OfflineManager` holds `Arc<CacheManager>` instead of separate `Arc<DiskCache>`. This avoids changing the `DiskCache` ownership model:

```rust
pub struct OfflineManager {
    pin_store: Arc<PinStore>,
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,  // Access disk via cache.disk
    drive_id: String,
    ttl_secs: std::sync::atomic::AtomicU64,
    max_folder_bytes: std::sync::atomic::AtomicU64,
}
```

#### Test Strategy

- **File**: `crates/carminedesktop-cache/tests/test_offline.rs` (NEW)
- Uses `wiremock` for Graph API mocking
- `test_offline_pin_folder_success()` — mock `get_item` (folder, size < limit), mock `list_children` + `download_content`, verify pin record created and files cached
- `test_offline_pin_folder_too_large()` — mock `get_item` with size > limit, verify `PinResult::Rejected`
- `test_offline_pin_file_rejected()` — mock `get_item` (file, not folder), verify rejection
- `test_offline_unpin_folder()` — pin then unpin, verify record removed
- `test_offline_process_expired()` — pin with TTL=0, call `process_expired()`, verify record removed
- `test_offline_redownload_changed_items()` — pin a folder, mock changed item with matching parent, verify re-download

---

### IPC Server (Windows)

- **Responsibility**: Named pipe server receiving pin/unpin requests from Explorer context menu verbs.
- **File(s)**: `crates/carminedesktop-app/src/ipc_server.rs` (NEW, `#[cfg(target_os = "windows")]`)
- **Implements**: CAP-04

#### Protocol

JSON-over-newline on `\\.\pipe\CarmineDesktop`:

**Request** (one JSON object per line, max 64 KB):
```json
{"action": "pin", "path": "C:\\Users\\user\\Cloud\\OneDrive\\Documents"}
```
```json
{"action": "unpin", "path": "C:\\Users\\user\\Cloud\\OneDrive\\Documents"}
```

**Response** (one JSON object):
```json
{"status": "ok"}
```
```json
{"status": "error", "message": "folder exceeds 5 GB limit"}
```

#### Struct and Interface

```rust
// crates/carminedesktop-app/src/ipc_server.rs

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
struct IpcRequest {
    action: String,
    path: String,
}

#[derive(Serialize)]
struct IpcResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub struct IpcServer {
    cancel: CancellationToken,
}

impl IpcServer {
    /// Start the named pipe server. Returns a handle that can be used to stop it.
    pub fn start(app: tauri::AppHandle) -> Self { ... }

    /// Stop the server.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}
```

#### Implementation

Uses `tokio::net::windows::named_pipe::{ServerOptions, NamedPipeServer}`:

```rust
pub fn start(app: tauri::AppHandle) -> Self {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tauri::async_runtime::spawn(async move {
        let pipe_name = r"\\.\pipe\CarmineDesktop";

        loop {
            let server = match ServerOptions::new()
                .first_pipe_instance(false)
                .create(pipe_name)
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("failed to create named pipe: {e}");
                    return;
                }
            };

            tokio::select! {
                _ = cancel_clone.cancelled() => break,
                result = server.connect() => {
                    match result {
                        Ok(()) => {
                            let app = app.clone();
                            tokio::spawn(async move {
                                handle_ipc_connection(server, app).await;
                            });
                        }
                        Err(e) => {
                            tracing::warn!("named pipe connect failed: {e}");
                        }
                    }
                }
            }
        }
    });

    Self { cancel }
}
```

The `handle_ipc_connection` function reads a line, parses JSON, dispatches to `resolve_item_for_path()` + `OfflineManager::pin_folder()` / `unpin_folder()`, and writes the response.

#### Integration Point

Started in `setup_after_launch()` (line 630), after `start_all_mounts()` (line 720) and before `start_delta_sync()` (line 746):

```rust
// Start IPC server for Explorer context menu (Windows only)
#[cfg(target_os = "windows")]
let _ipc_server = ipc_server::IpcServer::start(app.clone());
```

The `IpcServer` handle is stored in `AppState` for shutdown:

```rust
// In AppState (line 219):
#[cfg(target_os = "windows")]
pub ipc_server: Mutex<Option<ipc_server::IpcServer>>,
```

#### Test Strategy

- **File**: `crates/carminedesktop-app/tests/test_ipc_server.rs` (NEW, `#[cfg(target_os = "windows")]`)
- Integration test: start server, connect as client, send pin request, verify response
- Test invalid JSON → error response
- Test unknown action → error response
- Test oversized message → rejection

---

### CLI Arguments

- **Responsibility**: Accept `--offline-pin` and `--offline-unpin` from Explorer context menu verbs.
- **File(s)**: `crates/carminedesktop-app/src/main.rs` (MODIFY)
- **Implements**: CAP-05

#### Changes to `CliArgs` (line 173)

Add after `--open` (line 205):

```rust
/// Pin a folder for offline use (used by Explorer context menu)
#[arg(long)]
offline_pin: Option<String>,

/// Unpin a folder from offline use (used by Explorer context menu)
#[arg(long)]
offline_unpin: Option<String>,
```

#### Changes to Single-Instance Callback (line 524)

Current code handles `--open-online` and `--open`. Add `--offline-pin` and `--offline-unpin` after the existing handlers, following the same pattern:

```rust
// After the existing --open handler (line 545):
} else if let Some(pos) = argv.iter().position(|a| a == "--offline-pin")
    && let Some(path) = argv.get(pos + 1).cloned()
{
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        handle_offline_pin(&handle, path).await;
    });
} else if let Some(pos) = argv.iter().position(|a| a == "--offline-unpin")
    && let Some(path) = argv.get(pos + 1).cloned()
{
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        handle_offline_unpin(&handle, path).await;
    });
}
```

#### Handler Functions

```rust
#[cfg(feature = "desktop")]
async fn handle_offline_pin(app: &tauri::AppHandle, path: String) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    if !state.authenticated.load(std::sync::atomic::Ordering::Relaxed) {
        notify::offline_pin_rejected(app, &path, "sign in required");
        return;
    }

    match resolve_item_for_path_and_pin(app, &path).await {
        Ok(folder_name) => {
            notify::offline_pin_complete(app, &folder_name);
        }
        Err(e) => {
            let folder_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&path);
            notify::offline_pin_failed(app, folder_name, &e);
        }
    }
}

async fn resolve_item_for_path_and_pin(
    app: &tauri::AppHandle,
    path: &str,
) -> Result<String, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let (drive_id, item) = commands::resolve_item_for_path(&state, path).await?;

    let offline_mgr = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (_, _, _, mgr) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no active cache for drive '{drive_id}'"))?;
        mgr.clone()
    };

    let folder_name = item.name.clone();
    match offline_mgr.pin_folder(&item.id, &folder_name).await
        .map_err(|e| e.to_string())?
    {
        carminedesktop_cache::offline::PinResult::Ok => Ok(folder_name),
        carminedesktop_cache::offline::PinResult::Rejected { reason } => {
            notify::offline_pin_rejected(app, &folder_name, &reason);
            Err(reason)
        }
    }
}
```

Note: `resolve_item_for_path` in `commands.rs` is currently `async fn` with `&AppState`. It needs to be made `pub(crate)` (currently private) so `main.rs` handlers can call it.

#### Test Strategy

- Existing `test_cli_args_parse_all_options()` extended to include `--offline-pin` and `--offline-unpin`
- New test: `test_cli_args_offline_pin()` — parse `--offline-pin /path/to/folder`, verify field populated

---

### Context Menu Registration (Windows)

- **Responsibility**: Register/unregister Explorer context menu verbs for "Make available offline" and "Free up space".
- **File(s)**: `crates/carminedesktop-app/src/shell_integration.rs` (MODIFY, `#[cfg(target_os = "windows")]`)
- **Implements**: CAP-06

#### Constants

```rust
#[cfg(target_os = "windows")]
const CONTEXT_MENU_OFFLINE: &str = "CarmineDesktop.MakeOffline";
#[cfg(target_os = "windows")]
const CONTEXT_MENU_FREE_SPACE: &str = "CarmineDesktop.FreeSpace";
```

#### `register_context_menu()`

```rust
#[cfg(target_os = "windows")]
pub fn register_context_menu(mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    if mount_paths.is_empty() {
        return Ok(());
    }

    let exe_path = std::env::current_exe().map_err(|e| {
        carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell = hkcu.open_subkey_with_flags(
        r"Software\Classes\Directory\shell",
        KEY_READ | KEY_WRITE,
    )?;

    // Build AppliesTo AQS filter: OR-join mount paths
    let applies_to = mount_paths
        .iter()
        .map(|p| format!("System.ItemPathDisplay:~<\"{}\"", p))
        .collect::<Vec<_>>()
        .join(" OR ");

    // Register "Make available offline"
    {
        let (verb_key, _) = dir_shell.create_subkey(CONTEXT_MENU_OFFLINE)?;
        verb_key.set_value("MUIVerb", &"Make available offline")?;
        verb_key.set_value("AppliesTo", &applies_to)?;
        verb_key.set_value("Icon", &exe_str.as_ref())?;
        let (cmd_key, _) = verb_key.create_subkey("command")?;
        cmd_key.set_value("", &format!("\"{}\" --offline-pin \"%V\"", exe_str))?;
    }

    // Register "Free up space"
    {
        let (verb_key, _) = dir_shell.create_subkey(CONTEXT_MENU_FREE_SPACE)?;
        verb_key.set_value("MUIVerb", &"Free up space")?;
        verb_key.set_value("AppliesTo", &applies_to)?;
        verb_key.set_value("Icon", &exe_str.as_ref())?;
        let (cmd_key, _) = verb_key.create_subkey("command")?;
        cmd_key.set_value("", &format!("\"{}\" --offline-unpin \"%V\"", exe_str))?;
    }

    notify_shell_change();
    tracing::info!("registered offline context menu verbs");
    Ok(())
}
```

#### `unregister_context_menu()`

```rust
#[cfg(target_os = "windows")]
pub fn unregister_context_menu() -> carminedesktop_core::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell = match hkcu.open_subkey_with_flags(
        r"Software\Classes\Directory\shell",
        KEY_READ | KEY_WRITE,
    ) {
        Ok(k) => k,
        Err(e) => {
            tracing::debug!("could not open Directory\\shell: {e}, skipping");
            return Ok(());
        }
    };

    for verb in [CONTEXT_MENU_OFFLINE, CONTEXT_MENU_FREE_SPACE] {
        match dir_shell.delete_subkey_all(verb) {
            Ok(()) => tracing::debug!("removed context menu verb {verb}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("context menu verb {verb} not found, skipping");
            }
            Err(e) => {
                tracing::warn!("failed to remove context menu verb {verb}: {e}");
            }
        }
    }

    notify_shell_change();
    tracing::info!("unregistered offline context menu verbs");
    Ok(())
}
```

#### `update_context_menu_paths()`

```rust
#[cfg(target_os = "windows")]
pub fn update_context_menu_paths(mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    if mount_paths.is_empty() {
        return unregister_context_menu();
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell = hkcu.open_subkey_with_flags(
        r"Software\Classes\Directory\shell",
        KEY_READ | KEY_WRITE,
    )?;

    let applies_to = mount_paths
        .iter()
        .map(|p| format!("System.ItemPathDisplay:~<\"{}\"", p))
        .collect::<Vec<_>>()
        .join(" OR ");

    for verb in [CONTEXT_MENU_OFFLINE, CONTEXT_MENU_FREE_SPACE] {
        if let Ok(verb_key) = dir_shell.open_subkey_with_flags(verb, KEY_READ | KEY_WRITE) {
            verb_key.set_value("AppliesTo", &applies_to)?;
        }
    }

    notify_shell_change();
    Ok(())
}
```

#### Linux/macOS Stubs

```rust
#[cfg(not(target_os = "windows"))]
pub fn register_context_menu(_mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn unregister_context_menu() -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn update_context_menu_paths(_mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    Ok(())
}
```

#### Integration Points

**`setup_after_launch()`** (line 630) — after `start_all_mounts()` (line 720) and the nav pane block (line 743), before `run_crash_recovery()` (line 745):

```rust
// Register offline context menu verbs (Windows)
#[cfg(target_os = "windows")]
{
    let mount_paths: Vec<String> = {
        let config = state.effective_config.lock().unwrap();
        config.mounts.iter()
            .filter(|m| m.enabled)
            .map(|m| expand_mount_point(&m.mount_point))
            .collect()
    };
    if let Err(e) = shell_integration::register_context_menu(&mount_paths) {
        tracing::warn!("offline context menu registration failed: {e}");
    }
}
```

**`graceful_shutdown_without_exit()`** (line 1412) — before `stop_all_mounts()` (line 1425):

```rust
#[cfg(target_os = "windows")]
{
    if let Err(e) = shell_integration::unregister_context_menu() {
        tracing::warn!("offline context menu unregistration failed: {e}");
    }
}
```

#### Test Strategy

- **File**: existing `shell_integration.rs` `#[cfg(test)]` module (line 1103), add:
- `test_shell_integration_register_and_unregister_context_menu()` — register with mock paths, verify registry keys exist, unregister, verify removed
- `test_shell_integration_update_context_menu_paths()` — register, update paths, verify `AppliesTo` changed

---

### Configuration Changes

- **Responsibility**: Add offline TTL and max folder size settings.
- **File(s)**: `crates/carminedesktop-core/src/config.rs` (MODIFY)
- **Implements**: CAP-07

#### Constants (after line 8)

```rust
const DEFAULT_OFFLINE_TTL_SECS: u64 = 86400;       // 1 day
const DEFAULT_OFFLINE_MAX_FOLDER_SIZE: &str = "5GB";
const MIN_OFFLINE_TTL_SECS: u64 = 60;              // 1 minute
const MAX_OFFLINE_TTL_SECS: u64 = 604800;          // 7 days
```

#### `UserGeneralSettings` (line 152) — add after `explorer_nav_pane` (line 186)

```rust
/// How long pinned folders remain available offline (seconds).
/// Default: 86400 (1 day). Clamped to [60, 604800].
#[serde(default)]
pub offline_ttl_secs: Option<u64>,
/// Maximum folder size allowed for offline pinning (e.g. "5GB", "500MB").
/// Default: "5GB".
#[serde(default)]
pub offline_max_folder_size: Option<String>,
```

#### `EffectiveConfig` (line 226) — add after `explorer_nav_pane` (line 248)

```rust
/// How long pinned folders remain available offline (seconds).
pub offline_ttl_secs: u64,
/// Maximum folder size allowed for offline pinning.
pub offline_max_folder_size: String,
```

#### `EffectiveConfig::build()` (line 252) — add before the `Self { ... }` block (line 303)

```rust
let offline_ttl_secs = user_general
    .and_then(|g| g.offline_ttl_secs)
    .unwrap_or(DEFAULT_OFFLINE_TTL_SECS)
    .clamp(MIN_OFFLINE_TTL_SECS, MAX_OFFLINE_TTL_SECS);

let offline_max_folder_size = user_general
    .and_then(|g| g.offline_max_folder_size.clone())
    .unwrap_or_else(|| DEFAULT_OFFLINE_MAX_FOLDER_SIZE.to_string());
```

And add to the `Self { ... }` return:
```rust
offline_ttl_secs,
offline_max_folder_size,
```

#### `ConfigChangeEvent` (line 470) — add variants

```rust
OfflineTtlChanged(u64),
OfflineMaxFolderSizeChanged(String),
```

#### `diff_configs()` (line 495) — add after `notifications` check (line 522)

```rust
if old.offline_ttl_secs != new.offline_ttl_secs {
    events.push(ConfigChangeEvent::OfflineTtlChanged(new.offline_ttl_secs));
}
if old.offline_max_folder_size != new.offline_max_folder_size {
    events.push(ConfigChangeEvent::OfflineMaxFolderSizeChanged(
        new.offline_max_folder_size.clone(),
    ));
}
```

#### `reset_setting()` (line 60) — add cases

```rust
"offline_ttl_secs" => g.offline_ttl_secs = None,
"offline_max_folder_size" => g.offline_max_folder_size = None,
```

#### Test Strategy

- Extend existing config tests to cover new fields
- `test_config_offline_ttl_clamping()` — set to 0, verify clamped to 60; set to 999999, verify clamped to 604800
- `test_config_offline_defaults()` — verify defaults are 86400 and "5GB"
- `test_config_diff_offline_changes()` — change TTL, verify event emitted

---

### Notifications

- **Responsibility**: Desktop notifications for offline operations.
- **File(s)**: `crates/carminedesktop-app/src/notify.rs` (MODIFY)
- **Implements**: CAP-09

#### New Functions (append after `files_recovered`, line 168)

```rust
pub fn offline_pin_complete(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Available Offline",
        &format!("'{folder_name}' is now available offline"),
    );
}

pub fn offline_pin_rejected(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Offline Unavailable",
        &format!("Cannot make '{folder_name}' available offline: {reason}"),
    );
}

pub fn offline_pin_failed(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Offline Error",
        &format!("Failed to download '{folder_name}' for offline use: {reason}"),
    );
}

pub fn offline_unpin_complete(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Space Freed",
        &format!("'{folder_name}' is no longer pinned for offline use"),
    );
}
```

---

### Delta Sync Integration

- **Responsibility**: Pass changed items to `OfflineManager` and process expired pins.
- **File(s)**: `crates/carminedesktop-app/src/main.rs` (MODIFY)
- **Implements**: CAP-08

#### Changes to `start_delta_sync()` (line 1260)

The `SyncSnapshotRow` type alias (line 94) gains the `OfflineManager`:

```rust
type SyncSnapshotRow = (
    String,                                              // drive_id
    String,                                              // mount_id
    String,                                              // mount_name
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
    Arc<carminedesktop_cache::offline::OfflineManager>,  // NEW
);
```

The snapshot construction (line 1291) extracts the `OfflineManager` from `MountCacheEntry`:

```rust
caches
    .iter()
    .map(|(drive_id, (c, i, obs, offline))| {
        // ... existing code ...
        (
            drive_id.clone(),
            mount_id,
            mount_name,
            c.clone(),
            i.clone(),
            obs.clone(),
            offline.clone(),  // NEW
        )
    })
    .collect()
```

The `Ok(_result)` branch (line 1325) changes from:

```rust
Ok(_result) => {
    // Clear 403 state so the user is notified if access is lost again.
    notified_403.remove(drive_id.as_str());
}
```

To:

```rust
Ok(result) => {
    notified_403.remove(drive_id.as_str());

    // Re-download changed items in pinned folders
    if !result.changed_items.is_empty() {
        let offline = offline.clone();
        let changed = result.changed_items.clone();
        tokio::spawn(async move {
            if let Err(e) = offline.redownload_changed_items(&changed).await {
                tracing::warn!("offline re-download failed: {e}");
            }
        });
    }
}
```

After the per-drive loop (after line 1366), add TTL expiry processing:

```rust
// Process expired offline pins (once per cycle, not per drive)
for (_, _, _, _, _, _, offline) in &snapshot {
    if let Err(e) = offline.process_expired() {
        tracing::warn!("offline expiry processing failed: {e}");
    }
}
```

---

### Module Registration

- **File(s)**: `crates/carminedesktop-cache/src/lib.rs` (MODIFY)
- Add after line 6:

```rust
pub mod offline;
pub mod pin_store;
```

And add to exports:
```rust
pub use offline::OfflineManager;
pub use pin_store::PinStore;
```

- **File(s)**: `crates/carminedesktop-app/src/main.rs` (MODIFY)
- Add after `mod update;` (line 15):

```rust
#[cfg(all(feature = "desktop", target_os = "windows"))]
mod ipc_server;
```

---

## Data Flow

```
Explorer right-click → "Make available offline"
    │
    ▼
Registry verb: carminedesktop.exe --offline-pin "%V"
    │
    ▼
Single-instance plugin forwards argv to running instance
    │
    ▼
handle_offline_pin(app, path)
    │
    ├─ resolve_item_for_path(state, path) → (drive_id, DriveItem)
    │
    ├─ Get OfflineManager from mount_caches[drive_id]
    │
    ▼
OfflineManager::pin_folder(item_id, folder_name)
    │
    ├─ GraphClient::get_item() → validate is_folder(), check size
    │
    ├─ PinStore::pin(drive_id, item_id, ttl_secs)
    │
    ├─ tokio::spawn(recursive_download(...))
    │   │
    │   ├─ GraphClient::list_children() → walk tree
    │   │
    │   └─ GraphClient::download_content() → DiskCache::put()
    │
    └─ Return PinResult::Ok → notify::offline_pin_complete()

─── Delta Sync Loop (every 60s) ───

run_delta_sync() → DeltaSyncResult { changed_items }
    │
    ├─ OfflineManager::redownload_changed_items(changed_items)
    │   │
    │   └─ For items with pinned parent → re-download content
    │
    └─ OfflineManager::process_expired()
        │
        └─ PinStore::list_expired() → unpin each

─── Eviction ───

DiskCache::evict_if_needed()
    │
    ├─ For each LRU entry:
    │   │
    │   ├─ eviction_filter(drive_id, item_id)?
    │   │   │
    │   │   └─ PinStore::is_protected() → walk parent chain
    │   │
    │   ├─ Protected → skip
    │   └─ Not protected → evict
```

## Required Changes

| File | Nature of Change |
|------|-----------------|
| `crates/carminedesktop-cache/src/pin_store.rs` | **NEW** — PinStore struct with CRUD operations |
| `crates/carminedesktop-cache/src/offline.rs` | **NEW** — OfflineManager facade |
| `crates/carminedesktop-cache/src/lib.rs` | Add `pub mod pin_store; pub mod offline;` and re-exports |
| `crates/carminedesktop-cache/src/disk.rs` | Add `eviction_filter` field, `set_eviction_filter()`, modify `evict_if_needed()` |
| `crates/carminedesktop-cache/src/manager.rs` | Add `pin_store: Arc<PinStore>` field, create in `new()`, wire eviction filter |
| `crates/carminedesktop-cache/src/sqlite.rs` | Add `pinned_folders` DDL to `create_tables()` |
| `crates/carminedesktop-core/src/config.rs` | Add `offline_ttl_secs`, `offline_max_folder_size` to settings/config/events/diff |
| `crates/carminedesktop-app/src/main.rs` | Add CLI args, single-instance handlers, `MountCacheEntry` type change, delta sync integration, context menu registration in `setup_after_launch()`, unregistration in shutdown |
| `crates/carminedesktop-app/src/shell_integration.rs` | Add `register_context_menu()`, `unregister_context_menu()`, `update_context_menu_paths()` |
| `crates/carminedesktop-app/src/notify.rs` | Add 4 notification functions |
| `crates/carminedesktop-app/src/ipc_server.rs` | **NEW** — Windows named pipe server |
| `crates/carminedesktop-app/src/commands.rs` | Make `resolve_item_for_path()` `pub(crate)`, add offline fields to `SettingsInfo` |
| `crates/carminedesktop-cache/tests/test_pin_store.rs` | **NEW** — PinStore unit tests |
| `crates/carminedesktop-cache/tests/test_offline.rs` | **NEW** — OfflineManager integration tests |
| `crates/carminedesktop-cache/tests/test_disk_eviction.rs` | **NEW** — Eviction filter tests |

## Technical Decisions

- **PinStore uses a separate Connection**: Avoids contending with `SqliteStore`'s `Mutex<Connection>` on the eviction hot path. WAL mode supports concurrent readers safely. — **Reason**: Eviction filter is called per-entry during LRU sweep; blocking on the main SQLite mutex would serialize all cache operations.

- **Eviction filter is a callback, not a column**: A `pinned` column in `cache_entries` would require updating it for every file in a pinned folder tree (O(n) writes on pin/unpin). A callback queries `pinned_folders` + walks the `items` parent chain (O(depth) reads per eviction candidate). — **Reason**: Pins are rare; eviction candidates are checked one at a time. Read-heavy access pattern favors the callback approach.

- **OfflineManager holds `Arc<CacheManager>` not `Arc<DiskCache>`**: `DiskCache` is owned by `CacheManager` (not `Arc`-wrapped). Changing ownership would touch all call sites. — **Reason**: Minimal disruption to existing code; `OfflineManager` accesses `disk` via `cache.disk`.

- **Named pipe fallback, not primary IPC**: The single-instance plugin handles the common case (app already running). The named pipe is a fallback for edge cases where the plugin doesn't forward args. — **Reason**: Tauri's single-instance plugin is the idiomatic approach; the pipe adds robustness without replacing it.

- **Static registry verbs, not COM shell extension**: Static verbs under `Directory\shell\` with `AppliesTo` AQS filtering are simpler than a COM DLL shell extension. — **Reason**: No COM registration, no DLL to ship, no Explorer crash risk. The `AppliesTo` filter scopes verbs to VFS mount paths only.

- **TTL expiry piggybacks on delta sync timer**: No separate timer for TTL checks. `process_expired()` runs once per delta sync cycle (every 60s). — **Reason**: Avoids an additional timer; 60s granularity is acceptable for TTL expiry (pins last hours/days).

- **`redownload_changed_items()` checks immediate parent only**: For deeply nested files, the initial recursive download ensures all descendants are cached. Delta sync reports changed items with their current parent, which is sufficient to match against pinned folder IDs. — **Reason**: Walking the full parent chain for every changed item would be expensive. The immediate parent check is O(1) per item and correct for the common case.

- **No new error variants**: All offline errors map to existing `Error::Cache(String)` or `Error::Config(String)`. — **Reason**: The error enum already covers the needed categories. Adding variants would require updating all match arms across the codebase for a feature-specific concern.
