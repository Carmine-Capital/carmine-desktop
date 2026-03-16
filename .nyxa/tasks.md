# Tasks: Make Available Offline

## Summary

Add a folder pinning system with TTL-based expiry that allows users to mark VFS folders for persistent offline sync. Core pin management and download orchestration live in `carminedesktop-cache`, OS-specific shell integration (Windows context menu, IPC) lives in `carminedesktop-app`, and configuration extends `carminedesktop-core`.

## Phase 1: Configuration & Data Layer [M]

### T-01: Add offline settings to configuration [S]
- **Files**: `crates/carminedesktop-core/src/config.rs`
- **Implements**: CAP-07
- **Description**:
  Add constants `DEFAULT_OFFLINE_TTL_SECS` (86400), `DEFAULT_OFFLINE_MAX_FOLDER_SIZE` ("5GB"), `MIN_OFFLINE_TTL_SECS` (60), `MAX_OFFLINE_TTL_SECS` (604800). Add `offline_ttl_secs: Option<u64>` and `offline_max_folder_size: Option<String>` fields to `UserGeneralSettings`. Add computed `offline_ttl_secs: u64` and `offline_max_folder_size: String` fields to `EffectiveConfig` with clamping logic in `EffectiveConfig::build()`. Add `OfflineTtlChanged(u64)` and `OfflineMaxFolderSizeChanged(String)` variants to `ConfigChangeEvent`. Extend `diff_configs()` to detect changes. Extend `reset_setting()` to handle `"offline_ttl_secs"` and `"offline_max_folder_size"` keys.
- **Completion criterion**:
  `EffectiveConfig::build()` returns correct defaults and clamped values for offline settings. `diff_configs()` emits events when offline settings change. `reset_setting()` clears both new keys.
- [ ] **Status**

### T-02: Create PinStore with pinned_folders table [M]
- **Files**: `crates/carminedesktop-cache/src/pin_store.rs` (NEW), `crates/carminedesktop-cache/src/sqlite.rs`, `crates/carminedesktop-cache/src/lib.rs`
- **Implements**: CAP-01
- **Description**:
  Create `pin_store.rs` with `PinnedFolder` struct (`drive_id`, `item_id`, `pinned_at`, `expires_at` — all `String`) and `PinStore` struct wrapping `Mutex<Connection>`. Implement `PinStore::open(db_path)` which opens a new connection to the same DB file (WAL mode, `busy_timeout=5000`), without creating tables (table creation is in `SqliteStore`). Implement methods: `pin(drive_id, item_id, ttl_secs)` (upsert with `pinned_at = datetime('now')`, `expires_at = datetime('now', '+N seconds')`), `unpin(drive_id, item_id)` (DELETE, no error if missing), `is_pinned(drive_id, item_id)` (returns true if non-expired record exists), `list_expired()` (WHERE `expires_at < datetime('now')`), `list_all()` (all records), `is_protected(drive_id, item_id)` (walks parent chain via `items` table to check if any ancestor is pinned). Add `CREATE TABLE IF NOT EXISTS pinned_folders (...)` DDL to `SqliteStore::create_tables()`. Add `pub mod pin_store;` and re-export `PinStore` and `PinnedFolder` in `lib.rs`.
- **Completion criterion**:
  `PinStore` compiles, all methods execute correct SQL. Table is created alongside existing tables in `SqliteStore::open()`. Module is exported from the cache crate.
- [ ] **Status**

### T-03: Add eviction filter to DiskCache [S]
- **Files**: `crates/carminedesktop-cache/src/disk.rs`
- **Implements**: CAP-02
- **Description**:
  Add field `eviction_filter: std::sync::RwLock<Option<Arc<dyn Fn(&str, &str) -> bool + Send + Sync>>>` to `DiskCache`. Initialize to `RwLock::new(None)` in `DiskCache::new()`. Add `pub fn set_eviction_filter(&self, filter: Arc<dyn Fn(&str, &str) -> bool + Send + Sync>)` method. Modify `evict_if_needed()`: before the eviction loop, clone the filter from the `RwLock`. In the loop, for each `(drive_id, item_id, size)` candidate, if the filter returns `true`, skip the entry (do not evict). After the loop, if `freed < to_free`, log a warning: `"cache eviction could not free enough space: {freed}/{to_free} bytes freed, some entries are protected by offline pins"`.
- **Completion criterion**:
  When a filter is set, protected entries are skipped during eviction. When no filter is set, behavior is identical to current. Warning logged when eviction is insufficient due to protected entries.
- [ ] **Status**

## Phase 2: Offline Manager [M]

### T-04: Create OfflineManager facade [M]
- **Files**: `crates/carminedesktop-cache/src/offline.rs` (NEW), `crates/carminedesktop-cache/src/lib.rs`
- **Implements**: CAP-03, CAP-10
- **Description**:
  Create `offline.rs` with `OfflineManager` struct holding `Arc<PinStore>`, `Arc<GraphClient>`, `Arc<CacheManager>`, `drive_id: String`, `ttl_secs: AtomicU64`, `max_folder_bytes: AtomicU64`. Implement:
  - `new(pin_store, graph, cache, drive_id, ttl_secs, max_folder_bytes)` constructor.
  - `async fn pin_folder(&self, item_id: &str) -> Result<(), PinError>`: fetch `DriveItem` from cache (SQLite `get_item_by_id`) or Graph API (`get_item`), verify `is_folder()`, check `size <= max_folder_bytes` (re-fetch via `get_item()` if size == 0), insert pin record via `PinStore::pin()`, spawn background `recursive_download()` task, return immediately.
  - `async fn unpin_folder(&self, item_id: &str) -> Result<()>`: call `PinStore::unpin()`, do NOT delete cached files.
  - `async fn process_expired(&self) -> Result<Vec<PinnedFolder>>`: call `PinStore::list_expired()`, remove each via `unpin()`, return removed records.
  - `async fn redownload_changed_items(&self, changed: &[DriveItem]) -> Result<()>`: for each changed item, check if its `parent_reference.id` matches any pinned folder's `item_id` from `list_all()`, if so re-download via `GraphClient::download_content()` → `DiskCache::put()`.
  - `fn is_pinned(&self, item_id: &str) -> Result<bool>`: delegate to `PinStore::is_pinned()`.
  - `fn is_item_protected(&self, item_id: &str) -> Result<bool>`: delegate to `PinStore::is_protected()`.
  - `fn set_ttl_secs(&self, secs: u64)` and `fn set_max_folder_bytes(&self, bytes: u64)`: update atomics.
  - Define `PinError` enum: `FolderTooLarge { size: u64, max: u64 }`, `ItemNotFound`, `NotAFolder`, `AlreadyPinned`, `GraphError(carminedesktop_core::Error)`.
  - Private `async fn recursive_download(&self, item_id: &str)`: list children via `GraphClient::list_children()`, for each file download content and `disk.put()`, for each subfolder recurse.
  Add `pub mod offline;` and re-export `OfflineManager` and `PinError` in `lib.rs`.
- **Completion criterion**:
  `OfflineManager` compiles. `pin_folder()` validates size, inserts record, spawns download. `unpin_folder()` removes record. `process_expired()` cleans up. `redownload_changed_items()` filters and re-downloads. Module exported from cache crate.
- [ ] **Status**

### T-05: Wire PinStore and eviction filter into CacheManager [S]
- **Files**: `crates/carminedesktop-cache/src/manager.rs`
- **Implements**: CAP-01, CAP-02
- **Description**:
  Add `pub pin_store: Arc<PinStore>` field to `CacheManager`. In `CacheManager::new()`, after creating `SqliteStore` (which now creates the `pinned_folders` table), create `PinStore::open(&db_path)?` and wrap in `Arc`. After creating `DiskCache`, set the eviction filter: clone `pin_store`, call `disk.set_eviction_filter(Arc::new(move |drive_id, item_id| pin_store_clone.is_protected(drive_id, item_id).unwrap_or(false)))`. Store `pin_store` in the struct. Update `clear()` to also clear pinned_folders (optional — pins may survive cache clear).
- **Completion criterion**:
  `CacheManager` exposes `pin_store`. Eviction filter is wired at construction time. Pinned files are protected from LRU eviction.
- [ ] **Status**

## Phase 3: App Integration — Notifications, CLI, Delta Sync [M]

### T-06: Add offline notification functions [S]
- **Files**: `crates/carminedesktop-app/src/notify.rs`
- **Implements**: CAP-09
- **Description**:
  Add four public functions following the existing pattern (call `send()`):
  - `pub fn offline_pin_complete(app: &AppHandle, folder_name: &str)` — title: "Available Offline", body: "'{folder_name}' is now available offline."
  - `pub fn offline_pin_rejected(app: &AppHandle, folder_name: &str, size_gb: f64, max_gb: f64)` — title: "Folder Too Large", body: "'{folder_name}' is {size_gb:.1} GB. Maximum is {max_gb:.0} GB."
  - `pub fn offline_pin_failed(app: &AppHandle, folder_name: &str, reason: &str)` — title: "Offline Sync Failed", body: "Failed to make '{folder_name}' available offline: {reason}"
  - `pub fn offline_unpin_complete(app: &AppHandle, folder_name: &str)` — title: "Space Freed", body: "'{folder_name}' is no longer available offline."
- **Completion criterion**:
  Four functions compile and follow the existing notification pattern. Each produces a user-friendly message.
- [ ] **Status**

### T-07: Add CLI args and single-instance handler for offline pin/unpin [M]
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-05, CAP-08
- **Description**:
  Add `--offline-pin <path>` and `--offline-unpin <path>` to `CliArgs` (both `Option<String>`). In the `tauri_plugin_single_instance::init` callback, add pattern matching for `--offline-pin` and `--offline-unpin` after the existing `--open` handler. Each dispatches to a new async handler function.
  Add `handle_offline_pin(app: &AppHandle, path: String)` and `handle_offline_unpin(app: &AppHandle, path: String)`:
  - Resolve path to `(drive_id, item_id)` using the existing `resolve_item_for_path()` (make it `pub(crate)` if not already).
  - Look up `OfflineManager` from `mount_caches` (requires T-08 for the type change, but the handler code can be written now with a TODO).
  - Call `pin_folder()` / `unpin_folder()`.
  - On success/error, call the appropriate notification function from T-06.
  Extend `MountCacheEntry` type alias to include `Option<Arc<OfflineManager>>` as a 4th tuple element. Extend `SyncSnapshotRow` similarly.
  In the delta sync loop's `Ok(_result)` branch: if `result.changed_items` is not empty and the `OfflineManager` is `Some`, spawn `redownload_changed_items()`. After the per-drive loop, call `process_expired()` for each `OfflineManager`.
- **Completion criterion**:
  CLI args parse correctly. Single-instance callback dispatches pin/unpin. Delta sync loop calls `redownload_changed_items()` and `process_expired()`. `MountCacheEntry` and `SyncSnapshotRow` include `OfflineManager`.
- [ ] **Status**

### T-08: Wire OfflineManager into mount startup [M]
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-03, CAP-08
- **Description**:
  In `start_mount_common()` or the platform-specific `start_mount()` functions, after creating `CacheManager`, create an `OfflineManager` using `cache.pin_store.clone()`, `state.graph.clone()`, `cache.clone()`, `drive_id`, and config values (`offline_ttl_secs`, `offline_max_folder_size` parsed via `parse_cache_size()`). Wrap in `Arc`. Store in `MountCacheEntry` (4th tuple element). Update all places that destructure `MountCacheEntry` and `SyncSnapshotRow` to handle the new field (mount_caches insert, delta sync snapshot, `refresh_mount`, `clear_cache`, `run_crash_recovery`).
  In `save_settings()`, when offline config changes are detected, update each `OfflineManager`'s atomics via `set_ttl_secs()` / `set_max_folder_bytes()`.
- **Completion criterion**:
  Each mount creates an `OfflineManager`. The manager is accessible from `mount_caches`. Config changes propagate to running managers. All existing code that destructures `MountCacheEntry` compiles with the new field.
- [ ] **Status**

## Phase 4: Windows Shell Integration [M]

### T-09: Register context menu verbs for offline pin/unpin [M]
- **Files**: `crates/carminedesktop-app/src/shell_integration.rs`
- **Implements**: CAP-06
- **Description**:
  Add three new public functions (all `#[cfg(target_os = "windows")]` with Linux/macOS no-op stubs):
  - `pub fn register_context_menu(mount_roots: &[&std::path::Path]) -> carminedesktop_core::Result<()>`: Create two registry keys under `HKCU\Software\Classes\Directory\shell\`:
    - `CarmineDesktop.MakeOffline` with default value "Make available offline", `Icon` pointing to exe, `AppliesTo` set to an AQS filter matching mount root paths (e.g. `System.ItemPathDisplay:~<"C:\Users\...\Cloud"`), and `shell\command` set to `"<exe>" --offline-pin "%V"`.
    - `CarmineDesktop.FreeUpSpace` with default value "Free up space", same `AppliesTo` filter, and `shell\command` set to `"<exe>" --offline-unpin "%V"`.
    Call `SHChangeNotify(SHCNE_ASSOCCHANGED, ...)` after registration.
  - `pub fn unregister_context_menu() -> carminedesktop_core::Result<()>`: Delete both keys. Call `SHChangeNotify`. Silently ignore missing keys.
  - `pub fn update_context_menu_paths(mount_roots: &[&std::path::Path]) -> carminedesktop_core::Result<()>`: Update the `AppliesTo` value on both keys. Call `SHChangeNotify`.
  Linux/macOS stubs return `Ok(())`.
- **Completion criterion**:
  On Windows, two context menu entries appear for directories on VFS mounts. Entries invoke the app with `--offline-pin` / `--offline-unpin`. Non-Windows platforms compile with no-op stubs.
- [ ] **Status**

### T-10: Integrate context menu registration into app lifecycle [S]
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-06, CAP-11
- **Description**:
  In `setup_after_launch()`, after `start_all_mounts()` and the nav pane block, collect active mount root paths and call `shell_integration::register_context_menu(&mount_roots)`. Log warning on failure.
  In `graceful_shutdown_without_exit()`, call `shell_integration::unregister_context_menu()`. Log warning on failure, do not propagate error.
  In `sign_out()` (in `commands.rs`), call `shell_integration::unregister_context_menu()` before stopping mounts.
- **Completion criterion**:
  Context menu is registered after mounts start, unregistered on shutdown and sign-out. Errors are logged but not fatal.
- [ ] **Status**

## Phase 5: IPC Server (Windows) [M]

### T-11: Create Windows named pipe IPC server [M]
- **Files**: `crates/carminedesktop-app/src/ipc_server.rs` (NEW), `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-04
- **Description**:
  Create `ipc_server.rs` with `#[cfg(target_os = "windows")]` gate. Define `IpcServer` struct holding a `CancellationToken`. Implement:
  - `pub fn start(app: tauri::AppHandle) -> Self`: spawn a tokio task that creates a named pipe at `\\.\pipe\CarmineDesktop`, loops accepting connections, spawns a handler task per connection.
  - `handle_connection(app, pipe_stream)`: read a line (max 64KB), parse JSON `{"action": "pin"|"unpin", "path": "..."}`, resolve path to `(drive_id, item_id)` using `resolve_item_for_path()`, dispatch to `OfflineManager::pin_folder()` / `unpin_folder()`, write JSON response `{"status": "ok"}` or `{"status": "error", "message": "..."}`. 5s timeout per connection.
  - `pub fn stop(&self)`: cancel the token.
  Add `#[cfg(target_os = "windows")] mod ipc_server;` in `main.rs`.
  In `setup_after_launch()`, after context menu registration, start the IPC server and store the handle (add `ipc_server: Mutex<Option<ipc_server::IpcServer>>` field to `AppState` behind `#[cfg(target_os = "windows")]`).
  In `graceful_shutdown_without_exit()`, stop the IPC server.
- **Completion criterion**:
  On Windows, a named pipe server accepts JSON pin/unpin requests and responds with JSON status. Server starts with the app and stops on shutdown. Non-Windows platforms compile without the module.
- [ ] **Status**

## Phase 6: Settings UI & Commands [S]

### T-12: Expose offline settings in Tauri commands and frontend [S]
- **Files**: `crates/carminedesktop-app/src/commands.rs`
- **Implements**: CAP-07
- **Description**:
  Add `offline_ttl_secs: u64` and `offline_max_folder_size: String` fields to `SettingsInfo`. Populate from `EffectiveConfig` in `get_settings()`.
  Add `offline_ttl_secs: Option<u64>` and `offline_max_folder_size: Option<String>` parameters to `save_settings()`. Apply to `UserGeneralSettings` following the existing pattern. When these change, update running `OfflineManager` instances via `set_ttl_secs()` / `set_max_folder_bytes()`.
  Make `resolve_item_for_path()` `pub(crate)` if it isn't already (needed by T-07 and T-11).
- **Completion criterion**:
  `get_settings` returns offline TTL and max folder size. `save_settings` persists and applies changes. `resolve_item_for_path` is accessible from other modules in the app crate.
- [ ] **Status**

## Phase 7: Tests [M]

### T-13: PinStore unit tests [S]
- **Files**: `crates/carminedesktop-cache/tests/cache_tests.rs` (or new `test_pin_store.rs`)
- **Implements**: CAP-01
- **Description**:
  Add tests for PinStore:
  - `test_pin_store_pin_and_is_pinned`: pin a folder, verify `is_pinned()` returns true.
  - `test_pin_store_unpin`: pin then unpin, verify `is_pinned()` returns false.
  - `test_pin_store_unpin_nonexistent`: unpin a never-pinned folder, verify no error.
  - `test_pin_store_upsert_refreshes_timestamps`: pin twice, verify timestamps updated.
  - `test_pin_store_list_expired`: pin with TTL=0, verify it appears in `list_expired()`.
  - `test_pin_store_list_all`: pin multiple folders, verify all returned.
  Use `std::env::temp_dir()` for DB path with cleanup.
- **Completion criterion**:
  All PinStore tests pass. Edge cases (upsert, unpin nonexistent, expired) are covered.
- [ ] **Status**

### T-14: DiskCache eviction filter tests [S]
- **Files**: `crates/carminedesktop-cache/tests/cache_tests.rs` (or new `test_disk_eviction.rs`)
- **Implements**: CAP-02
- **Description**:
  Add tests for eviction filter:
  - `test_disk_eviction_skips_protected_entries`: set max size low, put two entries, set filter protecting one, trigger eviction, verify protected entry survives.
  - `test_disk_eviction_no_filter_unchanged`: verify eviction works identically when no filter is set.
  Use `std::env::temp_dir()` for cache dir and DB with cleanup.
- **Completion criterion**:
  Eviction filter correctly protects entries. No-filter behavior unchanged.
- [ ] **Status**

### T-15: OfflineManager integration tests [M]
- **Files**: `crates/carminedesktop-cache/tests/test_offline.rs` (NEW)
- **Implements**: CAP-03, CAP-10
- **Description**:
  Add integration tests using `wiremock` for Graph API mocking:
  - `test_offline_pin_folder_success`: mock `get_item` returning a folder with size < 5GB, mock `list_children` and `download_content`, call `pin_folder()`, verify pin record exists and files are cached.
  - `test_offline_pin_folder_too_large`: mock `get_item` returning a folder with size > 5GB, call `pin_folder()`, verify `PinError::FolderTooLarge` returned.
  - `test_offline_pin_not_a_folder`: mock `get_item` returning a file, verify `PinError::NotAFolder`.
  - `test_offline_unpin_folder`: pin then unpin, verify pin record removed, cached files still present.
  - `test_offline_process_expired`: pin with TTL=0, call `process_expired()`, verify record removed.
  Use `tokio::test`, `wiremock::MockServer`, temp dirs.
- **Completion criterion**:
  All OfflineManager tests pass. Size validation, pin/unpin lifecycle, and expiry are covered.
- [ ] **Status**
