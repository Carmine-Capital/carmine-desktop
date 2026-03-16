# Specifications: Make Available Offline

## Capabilities

### CAP-01: Pin Store — Persistent Pin Records

**Description**: A SQLite-backed store manages `pinned_folders` records in the existing per-mount database. Each record tracks a pinned folder's drive ID, item ID, pin timestamp, and expiry timestamp. The store provides CRUD operations and query methods for eviction protection and TTL expiry processing.

**Acceptance Criteria**:
- [ ] `pin()` inserts a record with `pinned_at = now` and `expires_at = now + ttl_secs`; upserting an existing pin refreshes both timestamps
- [ ] `unpin()` removes the record for a given `(drive_id, item_id)` pair; returns `Ok(())` even if no record exists
- [ ] `is_pinned(drive_id, item_id)` returns `true` if a non-expired record exists, `false` otherwise
- [ ] `list_expired()` returns all records where `expires_at < datetime('now')`
- [ ] `list_all()` returns all pinned folder records (for UI display and eviction filtering)
- [ ] `is_any_descendant_pinned(drive_id, item_id)` returns `true` if the given item_id appears in `pinned_folders` OR if any ancestor of a cached file is pinned (used by eviction filter)
- [ ] The `pinned_folders` table is created via `CREATE TABLE IF NOT EXISTS` during `SqliteStore::open()`, alongside existing tables
- [ ] All operations use the existing `Mutex<Connection>` pattern — no new connection pool

**Edge Cases**:
- Pin the same folder twice → upsert refreshes `pinned_at` and `expires_at`, no duplicate row
- Unpin a folder that was never pinned → no error, no-op
- `list_expired()` with no expired pins → returns empty `Vec`
- Database locked by concurrent writer → `busy_timeout=5000` handles contention (existing pragma)
- Pin a folder, then the folder is deleted server-side → pin record remains until TTL expiry or manual unpin; delta sync deletion does NOT auto-remove pin records (orphan cleanup is separate)

---

### CAP-02: Eviction Protection for Pinned Items

**Description**: The disk cache LRU eviction loop must skip files that belong to pinned folders. A callback predicate is injected into `DiskCache` that checks whether a given `(drive_id, item_id)` is protected by a pin. Protected items are never evicted, even when the cache exceeds `max_size_bytes`.

**Acceptance Criteria**:
- [ ] `DiskCache` accepts an optional eviction filter via `set_eviction_filter(filter: Arc<dyn Fn(&str, &str) -> bool + Send + Sync>)`
- [ ] `evict_if_needed()` calls the filter for each candidate entry; entries where `filter(drive_id, item_id)` returns `true` are skipped
- [ ] When no filter is set, eviction behaves identically to current behavior (all entries eligible)
- [ ] If all remaining entries are protected and cache still exceeds max, eviction stops gracefully (logs a warning, does not loop infinitely)
- [ ] The filter is set once during `CacheManager` construction (after `PinStore` is available)

**Edge Cases**:
- Cache is 100% pinned content exceeding max → eviction frees 0 bytes, logs warning, does not error
- Filter function panics → should not happen (simple SQLite query), but if it does, the entry is treated as unprotected (eviction proceeds)
- Pin is removed while eviction is in progress → next eviction cycle will correctly evict the now-unprotected items

---

### CAP-03: Offline Manager — Pin Lifecycle Orchestration

**Description**: The `OfflineManager` is the central facade for the offline feature. It validates folder size, creates pin records, spawns background recursive downloads, handles TTL expiry, and coordinates re-downloads when delta sync detects changes to pinned content.

**Acceptance Criteria**:
- [ ] `pin_folder(drive_id, item_id, folder_name)` validates the folder's `DriveItem.size` against `offline_max_folder_size`; rejects with a descriptive error if exceeded
- [ ] `pin_folder()` inserts a `PinStore` record, then spawns a background task to recursively download all file descendants
- [ ] Recursive download walks the folder tree via `GraphClient::list_children()`, downloading each file via `GraphClient::download_content()` and storing via `DiskCache::put()`
- [ ] `pin_folder()` returns immediately after spawning the download task (non-blocking)
- [ ] `unpin_folder(drive_id, item_id, folder_name)` removes the pin record; does NOT delete cached files (they become eligible for normal LRU eviction)
- [ ] `process_expired()` queries `PinStore::list_expired()`, removes each expired pin record, and logs the expiry
- [ ] `redownload_changed_items(drive_id, changed_items)` filters `changed_items` to those whose parent chain includes a pinned folder, then re-downloads their content
- [ ] Size validation uses the `DriveItem.size` field from the Graph API (folder size = sum of descendants), NOT local enumeration
- [ ] If the folder item has `size == 0` (Graph API sometimes returns 0 for folders), the manager fetches the item via `GraphClient::get_item()` to get an accurate size

**Edge Cases**:
- Pin a folder that is already pinned → upsert refreshes TTL, does NOT re-download (download task checks if content is already cached)
- Pin a folder with 0 files → pin record created, download task completes immediately
- Network failure during recursive download → partial download is retained in cache; next delta sync cycle will re-trigger download for missing files via `redownload_changed_items()`
- Folder exceeds size limit → `pin_folder()` returns `Err`, no pin record created, notification sent to user
- Pin a single file (not a folder) → rejected with error "only folders can be pinned"
- `process_expired()` called with no expired pins → no-op
- `redownload_changed_items()` called with empty `changed_items` → no-op

---

### CAP-04: IPC Server — Named Pipe Communication (Windows)

**Description**: A Windows named pipe server receives pin/unpin requests from Explorer context menu verbs. The pipe uses a JSON-over-newline protocol. The server runs as a background task in the Tauri async runtime.

**Acceptance Criteria**:
- [ ] Named pipe created at `\\.\pipe\CarmineDesktop` with a single-instance listener
- [ ] Accepts JSON messages: `{"action": "pin", "path": "<local_path>"}` and `{"action": "unpin", "path": "<local_path>"}`
- [ ] Responds with JSON: `{"status": "ok"}` on success, `{"status": "error", "message": "<reason>"}` on failure
- [ ] Each connection is handled in a spawned task (concurrent clients supported)
- [ ] The pipe server resolves `path` to `(drive_id, item_id)` using the existing `resolve_item_for_path()` pattern
- [ ] The pipe server is started in `setup_after_launch()` after mounts are active
- [ ] The pipe server is stopped during `graceful_shutdown()`
- [ ] Messages larger than 64 KB are rejected
- [ ] Invalid JSON or unknown actions return an error response (not a crash)

**Edge Cases**:
- Client connects but sends no data → timeout after 5 seconds, close connection
- Client sends partial JSON → parse error returned, connection closed
- Pipe server receives request before mounts are ready → returns error "mounts not ready"
- Multiple rapid connections → each handled independently via spawned tasks
- Named pipe already exists (stale from crashed process) → recreate pipe (Windows named pipes are cleaned up on last handle close)

---

### CAP-05: CLI Arguments — Offline Pin/Unpin

**Description**: Two new CLI arguments (`--offline-pin <path>` and `--offline-unpin <path>`) allow the Explorer context menu verbs to trigger pin/unpin operations. In single-instance mode, these are forwarded to the running instance via the Tauri single-instance plugin callback.

**Acceptance Criteria**:
- [ ] `--offline-pin <path>` is accepted by `clap::Parser` as an optional `String` argument
- [ ] `--offline-unpin <path>` is accepted by `clap::Parser` as an optional `String` argument
- [ ] When the app is already running, the single-instance plugin callback detects `--offline-pin` and `--offline-unpin` in `argv` and dispatches to the appropriate handler
- [ ] When the app is NOT running, the first instance processes the argument directly after `setup_after_launch()` completes
- [ ] The single-instance callback pattern matches the existing `--open-online` and `--open` handling (position-based argv scanning)
- [ ] If the path is not inside any active mount, an error notification is shown

**Edge Cases**:
- `--offline-pin` with a file path (not a folder) → error notification "only folders can be pinned"
- `--offline-pin` with a non-existent path → error notification "path not found"
- `--offline-pin` while not authenticated → error notification "sign in required"
- Both `--offline-pin` and `--offline-unpin` provided → only the first one is processed

---

### CAP-06: Context Menu Registration (Windows)

**Description**: Two static shell verbs are registered under `HKCU\Software\Classes\Directory\shell\` to add "Make available offline" and "Free up space" entries to the Windows Explorer right-click context menu for directories. The verbs are scoped to VFS mount paths via the `AppliesTo` AQS filter.

**Acceptance Criteria**:
- [ ] `register_context_menu(mount_paths)` creates two registry keys:
  - `HKCU\Software\Classes\Directory\shell\CarmineDesktop.MakeOffline` with `MUIVerb` = "Make available offline", `Icon` = exe path, `AppliesTo` = AQS filter matching mount paths, and `command` = `"<exe>" --offline-pin "%V"`
  - `HKCU\Software\Classes\Directory\shell\CarmineDesktop.FreeSpace` with `MUIVerb` = "Free up space", `Icon` = exe path, `AppliesTo` = AQS filter matching mount paths, and `command` = `"<exe>" --offline-unpin "%V"`
- [ ] `unregister_context_menu()` removes both registry key trees; missing keys are silently ignored
- [ ] `update_context_menu_paths(mount_paths)` updates the `AppliesTo` value without full unregister/register cycle
- [ ] `AppliesTo` uses `System.ItemPathDisplay:~<"<mount_path>"` AQS syntax, OR-joined for multiple mounts
- [ ] Registration is called in `setup_after_launch()` after `start_all_mounts()` succeeds
- [ ] Unregistration is called in `graceful_shutdown()` and sign-out flow
- [ ] `SHChangeNotify(SHCNE_ASSOCCHANGED)` is called after registration/unregistration

**Edge Cases**:
- No mounts active → context menu not registered (no mount paths to scope)
- Mount paths change (mount added/removed) → `update_context_menu_paths()` called with new paths
- Registry write fails (permissions) → error logged, app continues without context menu
- Unregister called when never registered → no error (idempotent)
- Mount path contains spaces or special characters → properly quoted in `AppliesTo` AQS and `command` value

---

### CAP-07: Configuration — Offline TTL and Max Folder Size

**Description**: Two new configuration fields control the offline feature: `offline_ttl_secs` (how long a pin lasts before auto-expiry) and `offline_max_folder_size` (maximum folder size allowed for pinning).

**Acceptance Criteria**:
- [ ] `UserGeneralSettings` gains `offline_ttl_secs: Option<u64>` and `offline_max_folder_size: Option<String>`, both `#[serde(default)]`
- [ ] `EffectiveConfig` gains `offline_ttl_secs: u64` (default 86400) and `offline_max_folder_size: String` (default "5GB")
- [ ] `EffectiveConfig::build()` clamps `offline_ttl_secs` to `[60, 604800]` (1 minute to 7 days)
- [ ] `ConfigChangeEvent` gains `OfflineTtlChanged(u64)` and `OfflineMaxFolderSizeChanged(String)` variants
- [ ] `diff_configs()` detects changes to both fields and emits the corresponding events
- [ ] `reset_setting("offline_ttl_secs")` and `reset_setting("offline_max_folder_size")` reset to `None`
- [ ] The `SettingsInfo` struct in `commands.rs` includes both fields for frontend display
- [ ] `save_settings` command accepts and persists both fields

**Edge Cases**:
- `offline_ttl_secs` set to 0 → clamped to 60
- `offline_ttl_secs` set to 999999 → clamped to 604800
- `offline_max_folder_size` set to invalid string (e.g. "abc") → `parse_cache_size()` returns default 5 bytes (existing behavior); should be validated at save time
- Config file missing `offline_ttl_secs` → defaults to 86400 via `unwrap_or()`
- Config change event for TTL → running `OfflineManager` updates its TTL for future pins (existing pins keep their original expiry)

---

### CAP-08: Delta Sync Integration — Re-download and TTL Expiry

**Description**: The existing delta sync loop is extended to (1) pass changed items to `OfflineManager::redownload_changed_items()` so pinned content stays current, and (2) call `OfflineManager::process_expired()` on each cycle to clean up expired pins.

**Acceptance Criteria**:
- [ ] In `start_delta_sync()`, the `Ok(_result)` branch passes `_result.changed_items` and the `drive_id` to `OfflineManager::redownload_changed_items()`
- [ ] In `start_delta_sync()`, after processing all drives in a cycle, `OfflineManager::process_expired()` is called once
- [ ] `redownload_changed_items()` only re-downloads items that are descendants of a pinned folder
- [ ] `process_expired()` removes expired pin records and logs each removal
- [ ] The delta sync loop does NOT block on re-downloads; they are spawned as background tasks
- [ ] If `OfflineManager` is not available (no mounts), the delta sync loop behaves identically to current behavior

**Edge Cases**:
- Delta sync reports a changed item that is in a pinned folder but the file was already re-downloaded → `DiskCache::put()` overwrites with new content (idempotent)
- Delta sync reports a deleted item that was in a pinned folder → pin record remains (folder still exists), deleted file is removed from cache normally
- TTL expires between two delta sync cycles → next cycle's `process_expired()` catches it
- All pins expired in one cycle → all removed, no crash

---

### CAP-09: Notifications — Offline Operation Feedback

**Description**: Desktop notifications inform the user about offline pin/unpin outcomes: completion, rejection (size limit), and failure (network error).

**Acceptance Criteria**:
- [ ] `offline_pin_complete(app, folder_name)` sends notification: title "Available Offline", body "'{folder_name}' is now available offline"
- [ ] `offline_pin_rejected(app, folder_name, reason)` sends notification: title "Offline Unavailable", body "Cannot make '{folder_name}' available offline: {reason}"
- [ ] `offline_pin_failed(app, folder_name, reason)` sends notification: title "Offline Error", body "Failed to download '{folder_name}' for offline use: {reason}"
- [ ] `offline_unpin_complete(app, folder_name)` sends notification: title "Space Freed", body "'{folder_name}' is no longer pinned for offline use"
- [ ] All notification functions follow the existing pattern: `pub fn <name>(app: &AppHandle, ...) { send(app, "Title", &body); }`
- [ ] Notifications respect the global `notifications` config toggle (existing `send()` function handles this)

**Edge Cases**:
- Notification system unavailable (e.g. headless mode) → `send()` logs warning, no crash
- Very long folder name → notification body may be truncated by OS, acceptable

---

### CAP-10: Size Validation — Folder Size Check

**Description**: Before pinning a folder, the system validates that the folder's total size (from `DriveItem.size`) does not exceed the configured maximum. This prevents users from accidentally pinning very large folders.

**Acceptance Criteria**:
- [ ] Size check uses `DriveItem.size` from the Graph API response (server-reported recursive size)
- [ ] If `DriveItem.size == 0` for a folder, the system fetches the item via `GraphClient::get_item()` to get an accurate size
- [ ] If the fetched size is still 0, the pin is allowed (empty folder or Graph API limitation)
- [ ] Size comparison uses `parse_cache_size()` to convert the config string (e.g. "5GB") to bytes
- [ ] Rejection message includes both the folder size and the limit in human-readable format (e.g. "2.3 GB exceeds the 1 GB limit")

**Edge Cases**:
- Folder size exactly equals the limit → allowed (comparison is `>`, not `>=`)
- Folder size is negative (Graph API bug) → treated as 0, pin allowed
- `offline_max_folder_size` config is "0" → all folders rejected (0 bytes limit)
- Network error fetching item size → pin rejected with "unable to verify folder size"

---

### CAP-11: Graceful Shutdown and Sign-Out Cleanup

**Description**: Context menu registry entries and the IPC server are cleaned up during graceful shutdown and sign-out to prevent stale Explorer entries.

**Acceptance Criteria**:
- [ ] `graceful_shutdown_without_exit()` calls `unregister_context_menu()` before stopping mounts
- [ ] Sign-out flow calls `unregister_context_menu()` alongside existing `unregister_file_associations()` and `unregister_nav_pane()`
- [ ] IPC server cancellation token is cancelled during shutdown
- [ ] If unregistration fails, shutdown continues (error logged, not fatal)

**Edge Cases**:
- Shutdown while a pin download is in progress → download task is cancelled via the sync cancellation token; partial cache content remains for future use
- Sign-out while pins exist → pin records remain in SQLite (they'll be irrelevant after sign-out since the DB is per-mount)
- Crash without graceful shutdown → stale registry entries remain; next launch's `register_context_menu()` overwrites them

---

### CAP-12: Error Handling

**Description**: All offline operations propagate errors through the existing `carminedesktop_core::Error` enum. No new error variants are needed; `Error::Cache(String)` covers pin store and offline manager errors.

**Acceptance Criteria**:
- [ ] Pin store errors use `Error::Cache(format!("pin store: {detail}"))`
- [ ] Offline manager errors use `Error::Cache(format!("offline: {detail}"))` for local errors and propagate `Error::GraphApi` / `Error::Network` for remote errors
- [ ] IPC server errors are logged and returned as JSON error responses; they do NOT crash the app
- [ ] Context menu registration errors use `Error::Config(String)` (consistent with existing shell integration)
- [ ] All error paths that reach the user produce a notification (via CAP-09 functions)
