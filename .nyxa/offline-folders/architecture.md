# Architecture: Make Available Offline

## Overview

The offline folder feature adds a pinning system that marks folders for persistent local caching with TTL-based expiry. The core pin management and download orchestration live in `carminedesktop-cache` (platform-agnostic), while OS-specific shell integration (Windows registry context menu, Linux D-Bus) lives in `carminedesktop-app`. The existing delta sync loop is extended with a hook to re-download changed pinned files, and the disk cache eviction is modified to skip pinned entries.

## Principles Applied

- **Single Responsibility**: Pin state management (`PinStore`) is separate from download orchestration (`OfflineManager`) and shell integration (context menu registration). Each has one reason to change.
- **Open/Closed**: The existing `DiskCache::evict_if_needed()` is extended with a predicate filter rather than rewritten. Delta sync gains a post-sync hook without modifying its core loop structure.
- **Dependency Inversion**: The app crate depends on abstractions from cache/core crates. The `OfflineManager` depends on `GraphClient` and `CacheManager` via `Arc` — no concrete platform knowledge.
- **Interface Segregation**: Shell integration exposes only `register_context_menu()` / `unregister_context_menu()`. The IPC server exposes only `pin_folder(path)` / `unpin_folder(path)`.
- **KISS/YAGNI**: No per-folder TTL override (uses global default). No progress UI (notifications only). No COM DLL (static registry verbs). SQLite table for pin state reuses the existing cache DB connection.

## Component Boundaries

### PinStore (carminedesktop-cache/src/pin_store.rs) — NEW MODULE

- **Responsibility**: Persistent storage of pin records. CRUD operations on the `pinned_folders` SQLite table.
- **Dependencies**: `rusqlite::Connection` (shared with `SqliteStore` via the same DB file), `carminedesktop_core::Result`
- **Key Interfaces**:
  ```rust
  pub struct PinRecord {
      pub drive_id: String,
      pub item_id: String,
      pub pinned_at: DateTime<Utc>,
      pub expires_at: DateTime<Utc>,
  }

  pub struct PinStore { conn: Mutex<Connection> }

  impl PinStore {
      pub fn new(conn: &Connection) -> Result<Self>;  // CREATE TABLE IF NOT EXISTS
      pub fn pin(&self, drive_id: &str, item_id: &str, ttl_secs: u64) -> Result<()>;
      pub fn unpin(&self, drive_id: &str, item_id: &str) -> Result<()>;
      pub fn is_pinned(&self, drive_id: &str, item_id: &str) -> Result<bool>;
      pub fn list_active(&self) -> Result<Vec<PinRecord>>;
      pub fn list_expired(&self) -> Result<Vec<PinRecord>>;
      pub fn remove_expired(&self) -> Result<Vec<PinRecord>>;  // returns removed for cleanup
      pub fn is_item_in_pinned_tree(&self, drive_id: &str, item_id: &str) -> Result<bool>;
  }
  ```
- **Data Model**: New table in the existing per-mount SQLite database:
  ```sql
  CREATE TABLE IF NOT EXISTS pinned_folders (
      drive_id TEXT NOT NULL,
      item_id  TEXT NOT NULL,
      pinned_at TEXT NOT NULL DEFAULT (datetime('now')),
      expires_at TEXT NOT NULL,
      PRIMARY KEY (drive_id, item_id)
  );
  ```
- **Design Note**: The `PinStore` shares the same `Connection` as `SqliteStore` (same DB file, same WAL pragmas). It gets its own `Mutex<Connection>` opened on the same path — consistent with how `DiskCache` already opens a separate connection to the same DB. This avoids coupling `PinStore` to `SqliteStore` internals.

### OfflineManager (carminedesktop-cache/src/offline.rs) — NEW MODULE

- **Responsibility**: Orchestrates the pin lifecycle — validates folder size, triggers recursive download, manages TTL expiry, and coordinates with delta sync for re-downloads.
- **Dependencies**: `Arc<GraphClient>`, `Arc<CacheManager>`, `PinStore`, `InodeTable`
- **Key Interfaces**:
  ```rust
  pub struct OfflineManager {
      graph: Arc<GraphClient>,
      cache: Arc<CacheManager>,
      pin_store: PinStore,
      max_folder_size: u64,       // from config, default 5 GB
      default_ttl_secs: u64,      // from config, default 86400 (1 day)
  }

  pub enum PinError {
      FolderTooLarge { size: u64, max: u64 },
      ItemNotFound,
      NotAFolder,
      AlreadyPinned,
      GraphError(carminedesktop_core::Error),
  }

  impl OfflineManager {
      pub fn new(graph, cache, pin_store, config) -> Self;
      pub async fn pin_folder(&self, drive_id: &str, item_id: &str) -> Result<(), PinError>;
      pub async fn unpin_folder(&self, drive_id: &str, item_id: &str) -> Result<()>;
      pub async fn process_expired(&self) -> Result<Vec<PinRecord>>;
      pub async fn redownload_changed_items(&self, drive_id: &str, changed: &[DriveItem]) -> Result<()>;
      pub fn is_pinned(&self, drive_id: &str, item_id: &str) -> Result<bool>;
      pub fn is_item_protected(&self, drive_id: &str, item_id: &str) -> Result<bool>;
  }
  ```
- **Pin Flow**:
  1. `pin_folder()` fetches `DriveItem` from cache/Graph, checks `folder.is_some()`, checks `size <= max_folder_size`
  2. Inserts pin record via `PinStore::pin()`
  3. Spawns `download_recursive()` — lists children via Graph API, downloads files to disk cache, recurses into subfolders
  4. Returns immediately after validation + pin record insert; download runs in background
  5. On completion/error, sends notification via event channel

### DiskCache Eviction Protection (carminedesktop-cache/src/disk.rs) — MODIFICATION

- **Responsibility**: Skip pinned files during LRU eviction.
- **Change**: `evict_if_needed()` accepts an optional predicate `is_protected: Option<&dyn Fn(&str, &str) -> bool>` that returns `true` for `(drive_id, item_id)` pairs that must not be evicted. The eviction loop skips entries where the predicate returns `true`.
- **Alternative considered**: Adding a `pinned` boolean column to `cache_entries`. Rejected because it couples disk cache tracking to pin semantics and requires schema migration. A callback predicate is simpler and more flexible.
- **Implementation**: `CacheManager` gains a `set_eviction_filter(filter: Arc<dyn Fn(&str, &str) -> bool + Send + Sync>)` method. The `DiskCache` stores this filter and consults it during eviction.

### Delta Sync Hook (carminedesktop-cache/src/sync.rs) — MODIFICATION

- **Responsibility**: After delta sync detects changed items, check if any belong to a pinned folder tree and re-download them.
- **Change**: `run_delta_sync()` already returns `DeltaSyncResult` with `changed_items`. The caller (app crate's delta sync loop) passes `changed_items` to `OfflineManager::redownload_changed_items()`.
- **Design Note**: The delta sync function itself is NOT modified. The integration point is in the app crate's delta sync loop, which already has access to the `DeltaSyncResult`. This respects the existing pattern where delta sync is a pure cache operation and the app layer decides what to do with results.

### TTL Expiry Timer (carminedesktop-app) — MODIFICATION

- **Responsibility**: Periodically check for expired pins and clean them up.
- **Change**: The existing delta sync timer loop in `start_delta_sync()` is extended to also call `OfflineManager::process_expired()` on each tick. This avoids a separate timer and reuses the existing interval (default 60s — sufficient granularity for TTL measured in days).
- **Alternative considered**: Separate `tokio::spawn` timer. Rejected — unnecessary complexity for a check that runs in microseconds (single SQLite query).

### IPC Server — Windows Named Pipe (carminedesktop-app/src/ipc_server.rs) — NEW MODULE

- **Responsibility**: Listen for pin/unpin requests from the Explorer context menu verb.
- **Platform**: `#[cfg(target_os = "windows")]`
- **Protocol**: Simple line-based JSON over a named pipe (`\\.\pipe\CarmineDesktop`).
  ```json
  {"action": "pin", "path": "C:\\Users\\...\\Cloud\\Documents\\Reports"}
  {"action": "unpin", "path": "C:\\Users\\...\\Cloud\\Documents\\Reports"}
  ```
  Response:
  ```json
  {"status": "ok"}
  {"status": "error", "message": "Folder too large (6.2 GB). Maximum is 5 GB."}
  ```
- **Dependencies**: `tokio::net::windows::named_pipe`, `AppState` (for mount resolution + `OfflineManager`)
- **Lifecycle**: Started in `setup_after_launch()` alongside delta sync. Stopped in `graceful_shutdown()`.
- **Design Note**: The registry verb command is `"<exe_path>" --offline-pin "%V"`. The `--offline-pin` CLI arg sends the path to the named pipe and exits. If the pipe is not available (app not running), the CLI prints an error and exits with code 1. This is the simplest approach — no second process, no COM.

### IPC Server — Linux D-Bus (carminedesktop-app/src/ipc_server.rs) — SAME MODULE

- **Responsibility**: Listen for pin/unpin requests from Nautilus/Dolphin scripts or CLI.
- **Platform**: `#[cfg(target_os = "linux")]`
- **Interface**: `com.carminedesktop.Desktop` on the session bus, method `PinFolder(path: String)` / `UnpinFolder(path: String)`.
- **Dependencies**: `zbus` crate (already common in Linux desktop Rust apps)
- **Design Note**: Linux context menu integration is best-effort. Nautilus scripts or Dolphin service menus can call `dbus-send` or `busctl`. No custom file manager extension.
- **YAGNI gate**: This is v1-optional. The `ipc_server.rs` module compiles on Linux only if a `dbus` feature flag is enabled. The architecture supports it but implementation can be deferred.

### Context Menu Registration (carminedesktop-app/src/shell_integration.rs) — MODIFICATION

- **Responsibility**: Register/unregister Windows Explorer context menu verbs for "Make available offline" and "Free up space".
- **Platform**: `#[cfg(target_os = "windows")]`
- **Registry Structure**:
  ```
  HKCU\Software\Classes\Directory\shell\CarmineDesktop.MakeOffline
      (Default) = "Make available offline"
      Icon = "<exe_path>,0"
      AppliesTo = "System.ItemPathDisplay:~<\"<mount_root>\""
      command\(Default) = "\"<exe_path>\" --offline-pin \"%V\""

  HKCU\Software\Classes\Directory\shell\CarmineDesktop.FreeSpace
      (Default) = "Free up space"
      Icon = "<exe_path>,0"
      AppliesTo = "System.ItemPathDisplay:~<\"<mount_root>\""
      command\(Default) = "\"<exe_path>\" --offline-unpin \"%V\""
  ```
- **Key Design Decisions**:
  - `AppliesTo` uses AQS (Advanced Query Syntax) to scope the menu to VFS mount paths only
  - Static verbs — no COM DLL, no dynamic show/hide based on pin state
  - Both verbs always appear; the app handles "already pinned" / "not pinned" gracefully
- **Functions**: `register_context_menu(mount_roots: &[&Path])`, `unregister_context_menu()`
- **Lifecycle**: Called from `setup_after_launch()` after mounts are started (so mount paths are known). Cleaned up in `unregister_file_associations()` or on uninstall.

### CLI Args (carminedesktop-app/src/main.rs) — MODIFICATION

- **New args**:
  ```rust
  #[arg(long)]
  offline_pin: Option<String>,    // path to pin
  #[arg(long)]
  offline_unpin: Option<String>,  // path to unpin
  ```
- **Behavior**: When `--offline-pin` or `--offline-unpin` is provided, the process connects to the named pipe (Windows) or D-Bus (Linux), sends the request, prints the response, and exits. It does NOT start the Tauri app.
- **Single-instance integration**: The existing `tauri_plugin_single_instance` forwards argv to the running instance. The single-instance handler checks for `--offline-pin` / `--offline-unpin` and dispatches to the `OfflineManager`. This is the primary path — the named pipe is a fallback for when single-instance forwarding is insufficient (e.g., the verb needs a synchronous response for error display).

### Notifications (carminedesktop-app/src/notify.rs) — MODIFICATION

- **New notification functions**:
  ```rust
  pub fn offline_pin_complete(app: &AppHandle, folder_name: &str);
  pub fn offline_pin_rejected(app: &AppHandle, folder_name: &str, reason: &str);
  pub fn offline_pin_failed(app: &AppHandle, folder_name: &str, reason: &str);
  pub fn offline_unpin_complete(app: &AppHandle, folder_name: &str);
  ```

### Configuration (carminedesktop-core/src/config.rs) — MODIFICATION

- **New settings in `UserGeneralSettings`**:
  ```rust
  #[serde(default)]
  pub offline_ttl_secs: Option<u64>,          // default: 86400 (1 day)
  #[serde(default)]
  pub offline_max_folder_size: Option<String>, // default: "5GB"
  ```
- **New fields in `EffectiveConfig`**:
  ```rust
  pub offline_ttl_secs: u64,           // default 86400, max 604800 (7 days)
  pub offline_max_folder_size: String,  // default "5GB"
  ```
- **Validation**: `EffectiveConfig::build()` clamps `offline_ttl_secs` to `[60, 604800]`.
- **Config change events**: `OfflineTtlChanged(u64)`, `OfflineMaxFolderSizeChanged(String)` added to `ConfigChangeEvent`.

### CacheManager (carminedesktop-cache/src/manager.rs) — MODIFICATION

- **New field**: `pub pin_store: PinStore`
- **Construction**: `CacheManager::new()` creates the `PinStore` alongside `SqliteStore` (same DB path).
- **New method**: `pub fn set_eviction_filter(&self, filter: Arc<dyn Fn(&str, &str) -> bool + Send + Sync>)` — stored in `DiskCache` for eviction protection.

## Dependency Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    carminedesktop-app                        │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ main.rs      │  │ commands.rs  │  │ shell_integration │  │
│  │ (CLI args,   │  │ (Tauri cmds) │  │ (context menu     │  │
│  │  IPC client) │  │              │  │  registry)        │  │
│  └──────┬───────┘  └──────┬───────┘  └───────────────────┘  │
│         │                 │                                  │
│  ┌──────┴─────────────────┴──────────────────────────────┐   │
│  │ ipc_server.rs (named pipe / D-Bus)                    │   │
│  │ → resolves path → calls OfflineManager                │   │
│  └───────────────────────┬───────────────────────────────┘   │
└──────────────────────────┼───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                   carminedesktop-cache                        │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ offline.rs   │  │ pin_store.rs │  │ disk.rs           │  │
│  │ (Offline     │──│ (PinStore    │  │ (eviction filter) │  │
│  │  Manager)    │  │  CRUD)       │  │                   │  │
│  └──────┬───────┘  └──────────────┘  └───────────────────┘  │
│         │                                                    │
│  ┌──────┴───────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ manager.rs   │  │ sqlite.rs    │  │ sync.rs           │  │
│  │ (CacheMgr    │  │ (items,      │  │ (delta sync)      │  │
│  │  + PinStore) │  │  delta_tokens)│  │                   │  │
│  └──────────────┘  └──────────────┘  └───────────────────┘  │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                   carminedesktop-core                         │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ config.rs    │  │ types.rs     │  │ error.rs          │  │
│  │ (offline_ttl,│  │ (DriveItem)  │  │ (Error enum)      │  │
│  │  max_size)   │  │              │  │                   │  │
│  └──────────────┘  └──────────────┘  └───────────────────┘  │
└──────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                   carminedesktop-graph                        │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ client.rs (list_children, download_content, get_item)│    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

Dependencies always point downward. No circular dependencies. `carminedesktop-cache` depends on `carminedesktop-core` and `carminedesktop-graph` (existing pattern). `carminedesktop-app` depends on all crates (existing pattern).

## Design Patterns

- **Observer (existing)**: `DeltaSyncObserver` trait in core, implemented by VFS. Extended: the app layer's delta sync loop now also notifies `OfflineManager` of changed items.
- **Strategy**: Eviction filter as a `dyn Fn` predicate injected into `DiskCache`. Decouples eviction policy from pin knowledge.
- **Command**: IPC messages are simple command objects (`{"action": "pin", "path": "..."}`) dispatched to `OfflineManager`.
- **Facade**: `OfflineManager` is the single entry point for all pin operations. Callers (IPC server, Tauri commands, delta sync hook) don't interact with `PinStore` or download logic directly.

## Trade-off Decisions

| Decision | Chosen | Alternative | Rationale |
|----------|--------|-------------|-----------|
| Pin state storage | SQLite table in existing per-mount DB | Separate JSON file | Atomic with other cache operations; consistent with existing pattern; supports queries (expired, by drive) |
| Eviction protection | Callback predicate on DiskCache | `pinned` column in `cache_entries` | Avoids schema migration; decouples concerns; predicate can check pin tree membership without denormalizing |
| IPC mechanism (Windows) | Named pipe + CLI arg | COM automation / WM_COPYDATA | Named pipe is simplest, works without GUI, supports structured responses. CLI arg via single-instance plugin is the primary path; pipe is fallback |
| IPC mechanism (Linux) | D-Bus session bus | Unix domain socket | D-Bus is the standard Linux desktop IPC; file manager scripts can use `dbus-send` directly |
| TTL expiry check | Piggyback on delta sync timer | Separate timer | One fewer timer; TTL granularity is days, 60s check interval is more than sufficient |
| Recursive download | Background task after pin record insert | Synchronous download before returning | User gets immediate feedback (notification); download can take minutes for large folders |
| Context menu scoping | `AppliesTo` AQS filter | Dynamic COM IContextMenu | Static verbs are dramatically simpler; AQS scoping to mount paths is sufficient |
| Pin granularity | Folder-level only | File-level | Intent specifies folder-level; file-level adds complexity without clear v1 value (YAGNI) |
| Re-download on delta change | App-layer hook after delta sync | Modify `run_delta_sync()` directly | Respects existing separation; delta sync stays a pure cache operation |
| New crate for offline logic | No — module in carminedesktop-cache | New carminedesktop-offline crate | Not enough code to justify a crate boundary; pin logic is inherently cache-adjacent |

## Constraints

- **Max folder size (5 GB)**: Checked at pin time via `DriveItem.size` (Graph API provides recursive folder size). Not re-checked on delta sync — if a pinned folder grows beyond 5 GB remotely, new files are still downloaded (the limit is a gate, not a cap).
- **Max TTL (7 days)**: Enforced in `EffectiveConfig::build()` via clamping. The UI can expose a slider but the backend enforces the ceiling.
- **No per-file pin tracking**: Only the root folder is recorded in `pinned_folders`. Membership in a pinned tree is determined by walking the SQLite `items` table parent chain. This is O(depth) per check but avoids maintaining a denormalized set of all descendant item IDs.
- **Single-instance constraint**: The Tauri single-instance plugin already handles argv forwarding. The `--offline-pin` arg is processed in the single-instance callback, which has access to `AppState`. The named pipe is a secondary path for cases where the single-instance plugin's fire-and-forget model is insufficient (the verb process needs a response to show an error).
- **CI zero-warnings**: All new code must pass `clippy --all-targets --all-features` with `-Dwarnings`. Platform-gated code must compile on all targets.
- **Existing `sync_state` table**: Dead code. Not repurposed — its schema doesn't match pin semantics. Left as-is (out of scope).

## Pin Lifecycle — Detailed Flow

```
User right-clicks folder in Explorer
        │
        ▼
Registry verb launches: carminedesktop.exe --offline-pin "C:\...\Reports"
        │
        ▼
CLI detects --offline-pin, connects to named pipe, sends {"action":"pin","path":"..."}
        │
        ▼
IPC server receives request
        │
        ▼
resolve_item_for_path(path) → (drive_id, DriveItem)
        │
        ▼
OfflineManager::pin_folder(drive_id, item_id)
        │
        ├── Check: item.is_folder()? → reject if not
        ├── Check: item.size <= max_folder_size? → reject if too large
        ├── Check: already pinned? → return AlreadyPinned
        ├── Insert PinRecord(drive_id, item_id, now, now + ttl)
        ├── Spawn background download task
        │       │
        │       ├── graph.list_children(drive_id, item_id) recursively
        │       ├── For each file: graph.download_content() → disk_cache.put()
        │       ├── On completion: notify::offline_pin_complete()
        │       └── On error: notify::offline_pin_failed()
        │
        └── Return Ok to IPC → CLI exits with success
        
        
Delta sync tick (every 60s):
        │
        ├── run_delta_sync() → DeltaSyncResult { changed_items }
        ├── offline_manager.redownload_changed_items(changed_items)
        │       └── For each changed item: if in pinned tree → re-download
        └── offline_manager.process_expired()
                ├── Query pinned_folders WHERE expires_at < now
                ├── Remove expired records
                └── (Files remain in cache but are now eligible for LRU eviction)


User right-clicks folder → "Free up space":
        │
        ▼
carminedesktop.exe --offline-unpin "C:\...\Reports"
        │
        ▼
OfflineManager::unpin_folder(drive_id, item_id)
        ├── Remove PinRecord
        ├── (Files remain in cache but are now eligible for LRU eviction)
        └── notify::offline_unpin_complete()
```

## Error Handling

| Failure Mode | Handling | Recovery |
|-------------|----------|----------|
| Folder too large | Reject pin, notify user with size info | User picks a smaller folder |
| Item not found in cache | Fetch from Graph API; if 404, notify user | User verifies folder exists |
| Network failure during download | Log error, notify user, pin record remains | Next delta sync tick retries; or user re-pins |
| Named pipe not available (app not running) | CLI prints error, exits with code 1 | User starts the app first |
| SQLite write failure | Propagate as `Error::Cache`, notify user | Transient — retry on next operation |
| Graph API 429 (throttled) | Existing retry/backoff in `GraphClient` handles this | Automatic retry with exponential backoff |
| Graph API 403 (access denied) | Reject pin, notify user | User checks permissions |
| Partial download (some files fail) | Download continues for remaining files; failures logged | Delta sync re-attempts on next tick |
| Pin record exists but download incomplete | On app restart, `process_active_pins()` checks completeness and resumes | Automatic recovery |

## Files Modified Summary

| Crate | File | Change Type |
|-------|------|-------------|
| carminedesktop-core | config.rs | Add `offline_ttl_secs`, `offline_max_folder_size` to settings |
| carminedesktop-core | config.rs | Add `ConfigChangeEvent` variants |
| carminedesktop-cache | pin_store.rs | **NEW** — PinStore with SQLite CRUD |
| carminedesktop-cache | offline.rs | **NEW** — OfflineManager orchestration |
| carminedesktop-cache | manager.rs | Add `pin_store` field, eviction filter |
| carminedesktop-cache | disk.rs | Add eviction filter predicate |
| carminedesktop-cache | lib.rs | Export new modules |
| carminedesktop-app | main.rs | Add CLI args, IPC client mode, start IPC server, extend delta sync loop |
| carminedesktop-app | ipc_server.rs | **NEW** — Named pipe (Windows) / D-Bus (Linux) server |
| carminedesktop-app | shell_integration.rs | Add context menu registration (Windows) |
| carminedesktop-app | notify.rs | Add offline notification functions |
| carminedesktop-app | commands.rs | Add Tauri commands for pin/unpin (UI integration, future) |
