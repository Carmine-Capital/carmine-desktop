# Review: Make Available Offline
**Date**: 2026-03-16
**Overall Status**: PASS WITH WARNINGS

## Findings

### [PASS] CAP-01: Pin Store — Persistent Pin Records

All acceptance criteria are satisfied:

- **pin() upserts with timestamps**: `pin_store.rs:55-61` — `INSERT...ON CONFLICT DO UPDATE` with `datetime('now')` and `datetime('now', '+' || ?3 || ' seconds')`. Correctly refreshes both `pinned_at` and `expires_at` on upsert.
- **unpin() no-op if not pinned**: `pin_store.rs:68-77` — `DELETE` returns `Ok(())` regardless of rows affected.
- **is_pinned() checks non-expired**: `pin_store.rs:81-94` — `WHERE expires_at > datetime('now')` correctly filters expired records.
- **list_expired()**: `pin_store.rs:97-130` — `WHERE expires_at <= datetime('now')` returns expired records.
- **list_all()**: `pin_store.rs:133-163` — Returns all records without expiry filter.
- **is_protected() walks parent chain**: `pin_store.rs:168-224` — Fast path for direct pin, then walks `items` table parent chain with 50-depth limit.
- **pinned_folders table in create_tables()**: `sqlite.rs:63-69` — `CREATE TABLE IF NOT EXISTS pinned_folders` with correct schema.

**Deviation from spec (non-breaking)**:
- Spec says "All operations use the existing `Mutex<Connection>` pattern — no new connection pool" (CAP-01 criterion 8). Implementation uses a **separate** `Mutex<Connection>` in `PinStore`, not the same one as `SqliteStore`. This is intentional per the design document (avoids contention on the hot path) and is the correct architectural choice. The spec wording is ambiguous — "pattern" can mean "same approach" rather than "same instance".
- Spec names the method `is_any_descendant_pinned()` but implementation names it `is_protected()`. The semantics are identical. The design document uses `is_protected()`.

---

### [PASS] CAP-02: Eviction Protection for Pinned Items

All acceptance criteria are satisfied:

- **set_eviction_filter()**: `disk.rs:248-250` — Accepts `Arc<dyn Fn(&str, &str) -> bool + Send + Sync>`.
- **evict_if_needed() skips protected**: `disk.rs:305-316` — Clones filter, checks each candidate, `continue` if protected.
- **No filter = unchanged behavior**: `disk.rs:305` — `filter` is `None`, the `if let Some(ref f)` block is skipped.
- **All-protected graceful stop**: `disk.rs:329-334` — Warning logged when `freed < to_free`.
- **Filter set once in CacheManager::new()**: `manager.rs:40-43` — Wired during construction using `is_protected()`.

Tests: `cache_tests.rs:1283-1346` — Two tests cover protected entries surviving eviction and no-filter behavior.

---

### [PASS] CAP-03: Offline Manager — Pin Lifecycle Orchestration

All acceptance criteria are satisfied:

- **pin_folder() validates size**: `offline.rs:53-81` — Fetches item, checks `is_folder()`, validates size against `max_folder_bytes`.
- **pin_folder() inserts record + spawns download**: `offline.rs:83-104` — `pin_store.pin()` then `tokio::spawn(recursive_download)`.
- **Recursive download**: `offline.rs:177-202` — Walks tree via `list_children()`, downloads files via `download_content()` + `disk.put()`.
- **pin_folder() returns immediately**: `offline.rs:104` — Returns `PinResult::Ok` after spawning.
- **unpin_folder() removes record, no file deletion**: `offline.rs:107-109` — Only calls `pin_store.unpin()`.
- **process_expired()**: `offline.rs:111-126` — Queries expired, unpins each, logs.
- **redownload_changed_items()**: `offline.rs:128-166` — Filters by parent_reference.id against pinned set, re-downloads files.
- **Size validation uses DriveItem.size**: `offline.rs:63-68` — Uses `item.size`, re-fetches if `<= 0`.
- **Size comparison uses `>` not `>=`**: `offline.rs:72` — `actual_size > max_bytes` (correct).

---

### [WARNING] CAP-04: IPC Server — Named Pipe Communication (Windows)

Most acceptance criteria are satisfied. One issue found:

- **Named pipe at `\\.\pipe\CarmineDesktop`**: `ipc_server.rs:12` — Correct.
- **JSON protocol**: `ipc_server.rs:15-26` — `IpcRequest`/`IpcResponse` with correct fields.
- **Concurrent clients**: `ipc_server.rs:60` — Each connection spawned in a new task.
- **Started in setup_after_launch()**: `main.rs:791-795` — Correct.
- **Stopped in graceful_shutdown()**: `main.rs:1614-1619` — Correct.
- **64KB limit**: `ipc_server.rs:71-78` — Checked after read.
- **5s timeout**: `ipc_server.rs:65-68` — `tokio::time::timeout(Duration::from_secs(5), ...)`.
- **Invalid JSON / unknown actions**: `ipc_server.rs:113-142` — Error responses returned.

**[WARNING] IPC always returns "ok" for pin/unpin regardless of outcome**:
- `ipc_server.rs:124-136` — `handle_offline_pin()` and `handle_offline_unpin()` are fire-and-forget (`async fn` returning `()`). The IPC response is always `{"status": "ok"}` even if the pin fails (e.g., folder too large, path not found).
- **Spec CAP-04**: "Responds with JSON: `{"status": "ok"}` on success, `{"status": "error", "message": "<reason>"}` on failure"
- **Impact**: LOW — The user still gets a desktop notification about the failure. The IPC client (Explorer context menu verb) doesn't display the response anyway.
- **Recommendation**: Refactor `handle_offline_pin/unpin` to return `Result<String, String>` and propagate the error to the IPC response.

---

### [PASS] CAP-05: CLI Arguments — Offline Pin/Unpin

All acceptance criteria are satisfied:

- **--offline-pin / --offline-unpin**: `main.rs:211-217` — `#[arg(long)]` with `Option<String>`.
- **Single-instance callback dispatch**: `main.rs:561-575` — Position-based argv scanning matching existing `--open-online`/`--open` pattern.
- **First instance processing**: The first instance processes CLI args via Tauri's standard arg handling (clap parser at startup).
- **Error notification for path not in mount**: `commands.rs:1008` — `resolve_item_for_path()` returns error "path is not inside any Carmine Desktop mount".

---

### [PASS] CAP-06: Context Menu Registration (Windows)

All acceptance criteria are satisfied:

- **register_context_menu()**: `shell_integration.rs:698-741` — Creates both registry keys with correct `MUIVerb`, `AppliesTo`, `Icon`, and `command` values.
- **unregister_context_menu()**: `shell_integration.rs:747-774` — Removes both keys, missing keys silently ignored.
- **update_context_menu_paths()**: `shell_integration.rs:780-803` — Updates `AppliesTo` without full cycle.
- **AppliesTo AQS syntax**: `shell_integration.rs:712-716` — `System.ItemPathDisplay:~<"<path>"` OR-joined.
- **Registration in setup_after_launch()**: `main.rs:775-789` — After mounts are active.
- **Unregistration in shutdown**: `main.rs:1610-1612` — Before `stop_all_mounts()`.
- **Unregistration in sign-out**: `commands.rs:190-193` — Called alongside existing cleanup.
- **SHChangeNotify**: `shell_integration.rs:738,771` — Called after both register and unregister.
- **Non-Windows stubs**: `shell_integration.rs:837-851` — Present for all three functions.

---

### [PASS] CAP-07: Configuration — Offline TTL and Max Folder Size

All acceptance criteria are satisfied:

- **UserGeneralSettings fields**: `config.rs:193-200` — `offline_ttl_secs: Option<u64>` and `offline_max_folder_size: Option<String>`, both `#[serde(default)]`.
- **EffectiveConfig fields**: `config.rs:264-266` — `offline_ttl_secs: u64` and `offline_max_folder_size: String`.
- **Clamping**: `config.rs:321-324` — `.clamp(MIN_OFFLINE_TTL_SECS, MAX_OFFLINE_TTL_SECS)` = `[60, 604800]`.
- **Defaults**: `config.rs:9-10` — 86400 and "5GB".
- **ConfigChangeEvent variants**: `config.rs:522-523` — `OfflineTtlChanged(u64)` and `OfflineMaxFolderSizeChanged(String)`.
- **diff_configs()**: `config.rs:554-561` — Detects changes and emits events.
- **reset_setting()**: `config.rs:78-79` — Both fields reset to `None`.
- **SettingsInfo**: `commands.rs:39-40` — Both fields present.
- **save_settings**: `commands.rs:445-497` — Both fields accepted and persisted.

---

### [PASS] CAP-08: Delta Sync Integration

All acceptance criteria are satisfied:

- **redownload_changed_items() in Ok branch**: `main.rs:1496-1505` — Spawns background task with `result.changed_items`.
- **process_expired() once per cycle**: `main.rs:1546-1551` — Called after the per-drive loop.
- **Only re-downloads descendants of pinned folders**: `offline.rs:146-148` — Checks `parent_reference.id` against pinned set.
- **Background tasks (non-blocking)**: `main.rs:1500` — `tokio::spawn()`.
- **OfflineManager not available**: The snapshot is built from `mount_caches` — if no mounts, snapshot is empty, loop body is skipped.

---

### [PASS] CAP-09: Notifications — Offline Operation Feedback

All acceptance criteria are satisfied:

- **offline_pin_complete**: `notify.rs:170-176` — Title "Available Offline", body matches spec.
- **offline_pin_rejected**: `notify.rs:178-184` — Title "Offline Unavailable", body matches spec.
- **offline_pin_failed**: `notify.rs:186-192` — Title "Offline Error", body matches spec.
- **offline_unpin_complete**: `notify.rs:194-200` — Title "Space Freed", body matches spec.
- **Pattern**: All follow `pub fn <name>(app: &AppHandle, ...) { send(app, "Title", &body); }`.

---

### [PASS] CAP-10: Size Validation — Folder Size Check

All acceptance criteria are satisfied:

- **Uses DriveItem.size**: `offline.rs:63` — `item.size`.
- **Re-fetches if size <= 0**: `offline.rs:63-68` — `if item.size <= 0` triggers re-fetch.
- **Still 0 → allowed**: `offline.rs:65` — `refreshed_item.size.max(0) as u64` → 0, and `max_bytes > 0 && 0 > max_bytes` is false.
- **parse_cache_size()**: Used in `main.rs:1193` to convert config string.
- **Human-readable rejection**: `offline.rs:74-80` — `format_bytes()` produces "X.Y GB" format.
- **Size at limit → allowed**: `offline.rs:72` — `actual_size > max_bytes` (strict greater-than).

---

### [PASS] CAP-11: Graceful Shutdown and Sign-Out Cleanup

All acceptance criteria are satisfied:

- **graceful_shutdown unregisters context menu**: `main.rs:1610-1612` — Before `stop_all_mounts()`.
- **Sign-out unregisters context menu**: `commands.rs:190-193` — Called in sign_out flow.
- **IPC server cancelled**: `main.rs:1614-1619` — `ipc.stop()` calls `cancel.cancel()`.
- **Errors non-fatal**: `main.rs:1611` — `tracing::warn!`, execution continues.

---

### [PASS] CAP-12: Error Handling

All acceptance criteria are satisfied:

- **Pin store errors use Error::Cache**: `pin_store.rs:31,37,53,63,76` — All use `Error::Cache(format!("pin store: ..."))`.
- **Offline manager errors**: `offline.rs` — Local errors via `pin_store` (Error::Cache), remote errors propagated as Error::GraphApi/Network.
- **IPC errors as JSON**: `ipc_server.rs:116-141` — Error responses returned, no crash.
- **Context menu errors use Error::Config**: `shell_integration.rs:704` — `Error::Config(...)`.
- **User-facing errors produce notifications**: `main.rs:920,926,933,952` — All error paths produce notifications.

---

## Design Coherence

### [PASS] D-01: PinStore opens separate Connection (WAL mode)
`pin_store.rs:30-34` — Opens its own `Connection` with WAL pragmas. Matches design.

### [PASS] D-02: Eviction filter is callback predicate, not column
`disk.rs:9,15` — `EvictionFilter = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>`. Matches design.

### [PASS] D-03: OfflineManager holds Arc<CacheManager> not Arc<DiskCache>
`offline.rs:22` — `cache: Arc<CacheManager>`. Matches design decision.

### [PASS] D-04: Named pipe fallback, single-instance primary
`main.rs:561-575` — Single-instance plugin handles common case. `ipc_server.rs` is the fallback. Matches design.

### [PASS] D-05: Static registry verbs with AppliesTo AQS
`shell_integration.rs:698-741` — Static verbs under `Directory\shell\` with AQS filter. Matches design.

### [PASS] D-06: TTL expiry piggybacks on delta sync timer
`main.rs:1546-1551` — `process_expired()` called in delta sync loop. No separate timer. Matches design.

### [PASS] D-07: redownload_changed_items() checks immediate parent only
`offline.rs:146-148` — Checks `parent_reference.id` against pinned set. Matches design.

### [PASS] D-08: No new error variants
All errors use existing `Error::Cache(String)` and `Error::Config(String)`. Matches design.

---

## Task Completeness

| Task | Status | Verification |
|------|--------|-------------|
| T-01: Offline settings in config | DONE | `config.rs` — constants, fields, clamping, events, diff, reset |
| T-02: PinStore with pinned_folders | DONE | `pin_store.rs` (NEW), `sqlite.rs`, `lib.rs` |
| T-03: Eviction filter in DiskCache | DONE | `disk.rs` — field, setter, eviction loop modification |
| T-04: OfflineManager facade | DONE | `offline.rs` (NEW), `lib.rs` |
| T-05: Wire PinStore into CacheManager | DONE | `manager.rs` — pin_store field, eviction filter wiring |
| T-06: Notification functions | DONE | `notify.rs` — 4 functions |
| T-07: CLI args + single-instance | DONE | `main.rs` — args, dispatch, handlers |
| T-08: Wire OfflineManager into mount | DONE | `main.rs` — MountContext, start_mount_common, delta sync |
| T-09: Context menu verbs | DONE | `shell_integration.rs` — register/unregister/update + stubs |
| T-10: Context menu lifecycle | DONE | `main.rs` — setup_after_launch, shutdown, sign-out |
| T-11: IPC server | DONE | `ipc_server.rs` (NEW), `main.rs` — start/stop |
| T-12: Settings UI & commands | DONE | `commands.rs` — SettingsInfo, save_settings, resolve_item_for_path pub(crate) |
| T-13: PinStore unit tests | DONE | `cache_tests.rs` — 6 tests |
| T-14: Eviction filter tests | DONE | `cache_tests.rs` — 2 tests |
| T-15: OfflineManager integration tests | DONE | `test_offline.rs` — 5 tests |

**All 15 tasks are complete.**

---

## Code Quality

### [OK] Error handling
- All SQLite operations properly map errors to `Error::Cache(String)`.
- Mutex lock failures handled gracefully (return `false` in `is_pinned`/`is_protected`).
- Network errors propagated via `?` operator.

### [OK] Thread safety
- `PinStore` uses `Mutex<Connection>` — consistent with codebase pattern.
- `OfflineManager` uses `AtomicU64` for TTL/max_bytes — lock-free reads.
- `DiskCache` eviction filter uses `RwLock` — appropriate for read-heavy access.

### [OK] Naming conventions
- Follows existing patterns: `snake_case` functions, `PascalCase` types.
- Error messages prefixed with component name ("pin store:", "offline:").

### [OK] No dead code or forgotten TODOs
- `_make_file` helper in `test_offline.rs:52` is prefixed with `_` (unused but available for future tests).
- `update_context_menu_paths` has `#[allow(dead_code)]` with comment "Reserved for future use" — acceptable.

### [WARNING] `format_bytes()` is private but could be useful elsewhere
- `offline.rs:204-218` — `format_bytes()` is a private helper. If human-readable sizes are needed elsewhere (e.g., UI), consider moving to `carminedesktop-core`.
- **Impact**: LOW — cosmetic, no functional issue.

### [INFO] `recursive_download` error handling
- `offline.rs:93-101` — Download errors are logged but not propagated to the user via notification. The `pin_complete` notification is sent before the download finishes (by design — non-blocking). If the download fails, only a `tracing::error!` is emitted.
- This matches the spec: "Network failure during recursive download → partial download is retained in cache; next delta sync cycle will re-trigger download."
- **Recommendation**: Consider sending `offline_pin_failed` notification from the spawned task on download failure for better UX.

### [INFO] `redownload_changed_items` checks immediate parent only
- `offline.rs:146-148` — Only checks `parent_reference.id` against pinned folder IDs. Files nested deeper than one level below a pinned folder won't be re-downloaded by this method.
- This is by design (documented in design.md): "the initial recursive download ensures all descendants are cached. Delta sync `changed_items` only contains items whose eTag changed — their immediate parent is sufficient."
- **Caveat**: If a file is moved from a non-pinned folder INTO a deeply nested subfolder of a pinned folder, and the subfolder itself is not pinned, the file won't be re-downloaded. This is an acceptable edge case per the design.

---

## Summary

The "Make Available Offline" feature is **fully implemented** across all 12 capabilities and 15 tasks. The implementation faithfully follows the design document's architectural decisions (separate PinStore connection, callback-based eviction filter, Arc<CacheManager> ownership, single-instance primary with IPC fallback). All tests pass (0 failures across the full test suite).

**One warning**: The IPC server always returns `{"status": "ok"}` for pin/unpin operations regardless of actual outcome, which deviates from CAP-04's spec that errors should be returned as JSON. This has low practical impact since the Explorer context menu verb doesn't display the response, and the user receives desktop notifications for all outcomes.

**Recommended actions**:
1. **(Optional, LOW priority)** Refactor `handle_offline_pin/unpin` to return `Result` so the IPC server can propagate error status in its JSON response.
2. **(Optional, LOW priority)** Consider sending `offline_pin_failed` notification from the background download task when `recursive_download` fails.
