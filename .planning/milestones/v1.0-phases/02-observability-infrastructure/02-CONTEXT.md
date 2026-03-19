# Phase 2: Observability Infrastructure - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Build the event bus, error accumulator, stat methods, and Tauri commands that power the dashboard. All sync state, activity, errors, and cache metrics must be queryable from the backend and testable from the browser console before any UI work (Phase 3). No frontend UI in this phase — data layer only.

Success criteria (from ROADMAP.md):
1. `invoke("get_dashboard_status")` → per-drive sync state, online/offline, last synced, auth health
2. `invoke("get_recent_errors")` → recent errors with file name, error type, timestamp
3. `invoke("get_cache_stats")` → disk cache usage vs max, pinned item count, writeback queue contents
4. Real-time events pushed via `emit()`, verifiable with `listen()` in browser console

</domain>

<decisions>
## Implementation Decisions

### Error visibility threshold
- **Only persistent failures** reach the dashboard error log — transient errors that auto-retry and succeed stay in log files only
- **Auth degradation is a state, not an error** — surfaced via the degraded auth banner (DASH-04), not as an error log entry
- **Error sources: VFS events + delta sync errors** — both VfsEvent types (ConflictDetected, WritebackFailed, UploadFailed, FileLocked) and delta sync failures (drive deleted, permission denied, item-level Graph failures) feed the accumulator
- **Include actionable hints per error type** — each error entry carries a short action string (e.g., "Re-authenticate", "File locked by user@contoso.com — try again later") that Phase 3 can display alongside the error

### Activity feed granularity
- **Individual entries per file** — a delta sync touching 150 files produces 150 activity entries, not 1 batch summary. Most recent entries at top.
- **Tag each entry by type** — entries are tagged as "uploaded" (user-initiated write), "synced" (downloaded from server via delta sync), "deleted", or "conflict". Users can distinguish their changes from others'.
- **Full remote path per entry** — store the complete path from DriveItem (e.g., "/Documents/Reports/Q4.xlsx"), not just the file name. Phase 3 can truncate for display but the data disambiguates same-named files.
- **Files only** — folder create/delete operations do not appear in the activity feed. Users care about content files.

### Offline pin health definition
- **Stale** = server content has changed since the pin was last synced. Requires storing a "last pinned at" timestamp per pin and comparing against server-side last-modified.
- **Partial** = not all files in the pinned directory tree are present in disk cache. Determined by walking the pinned folder's metadata in SQLite and checking for disk cache entries.
- **Downloaded** = all files present in disk cache and no server-side changes since last pin sync.
- **Health computed on-demand** — assessed when `get_cache_stats` is called, not maintained as running state. Always accurate, no background overhead.
- **Include file count breakdown** — return `total_files` and `cached_files` per pin so Phase 3 can show "47/52 files downloaded" or a percentage.

### Event retention & lifecycle
- **Error buffer: 100 entries** — ring buffer, oldest dropped when full
- **Activity buffer: 500 entries** — ring buffer, oldest dropped when full. A 150-file sync uses ~30% of the buffer.
- **Return all, filter client-side** — `get_recent_errors` and activity feed commands return the full buffer. With 100/500 entries, payloads are small. Phase 3 filters in JS.
- **In-memory only** — buffers cleared on app restart. Log files are the persistent record. Dashboard shows current session activity.

### Claude's Discretion
- Event bus implementation details (broadcast channel topology, subscriber management)
- Error accumulator internal data structures (ring buffer implementation, locking strategy)
- Cache stat method implementations on MemoryCache, DiskCache, CacheManager
- Tauri command naming conventions and response struct shapes (beyond what success criteria specify)
- Real-time event throttling/batching approach (how frequently emit() fires)
- Whether to use a custom tracing Layer for error capture or explicit error forwarding
- SyncHandle exposure strategy (stored in MountCacheEntry vs. separate channel)
- Per-mount sync state enum variants and transition logic

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### VFS event system (extend, don't replace)
- `crates/carminedesktop-vfs/src/core_ops.rs` — `VfsEvent` enum (line 346): ConflictDetected, WritebackFailed, UploadFailed, FileLocked. `event_tx` mpsc channel (line 480). Currently single-consumer (notifications only) — must become multi-consumer for dashboard.
- `crates/carminedesktop-vfs/src/sync_processor.rs` — `SyncMetrics` struct (line 57): queue_depth, in_flight, failed_count, total_uploaded, total_failed, total_deduplicated. `SyncHandle::metrics()` via watch channel (line 93). Already computed per tick — needs exposure to AppState.

### Tauri command infrastructure (add new commands here)
- `crates/carminedesktop-app/src/commands.rs` — 25 existing commands. Pattern: `app.state::<AppState>()`, return `Result<T, String>`. New dashboard commands register in `invoke_handler!`.
- `crates/carminedesktop-app/src/main.rs` — `AppState` struct (line 235): holds `mount_caches`, `mounts`, `auth_degraded`, `offline_flag` per mount. Command registration at line 612. Tracing setup at line 431 (layered registry — can add custom Layer).

### Cache stats (add stat methods)
- `crates/carminedesktop-cache/src/manager.rs` — `CacheManager` facade. Fields: memory, sqlite, disk, writeback, pin_store. No stats method yet.
- `crates/carminedesktop-cache/src/disk.rs` — `DiskCache::total_size()` (line 252) returns bytes. `max_size_bytes` is configurable limit. No entry_count().
- `crates/carminedesktop-cache/src/memory.rs` — `MemoryCache::entries` is DashMap with `.len()`. No public stats method.
- `crates/carminedesktop-cache/src/writeback.rs` — `WriteBackBuffer::list_pending()` (line 164) returns pending uploads. In-memory + disk persistence.

### Pin health (extend PinStore)
- `crates/carminedesktop-cache/src/pin_store.rs` — `PinStore` with `is_pinned()` (line 81), `is_protected()` (line 168). No health assessment, no last-pinned timestamp.
- `crates/carminedesktop-cache/src/offline.rs` — `OfflineManager` and `recursive_download()` (line 193). Downloads content + populates SQLite metadata (Phase 1 fix). Entry point for pin completion tracking.

### Delta sync (error source + activity source)
- `crates/carminedesktop-app/src/main.rs` — `start_delta_sync()` (line 1496): handles per-drive errors (404, 403, auth degradation, network). Returns `DeltaSyncResult` with changed_items and deleted_items.
- `crates/carminedesktop-cache/src/sync.rs` — `DeltaSyncResult` (line 66): `changed_items: Vec<DriveItem>`, `deleted_items: Vec<DeletedItemInfo>`. Source for activity feed entries.

### Existing event emission (pattern to follow)
- `crates/carminedesktop-app/src/notify.rs` — 20 notification functions using `app.notification().builder()`. Error types here map 1:1 to dashboard error categories.
- `crates/carminedesktop-app/src/tray.rs` — `app.emit()` pattern for backend→frontend events.

### Frontend IPC (Phase 3 will consume)
- `crates/carminedesktop-app/dist/settings.js` — `invoke()` and `listen()` patterns. `init()` uses `Promise.all()` for parallel data fetch.
- `crates/carminedesktop-app/dist/ui.js` — `showStatus()` and `formatError()` — existing feedback patterns Phase 3 extends.

### Prior phase context
- `.planning/phases/01-winfsp-offline-pin-fix/01-CONTEXT.md` — Phase 1 deferred "custom offline error categories" to this phase. VFS timeout + offline detection patterns established.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`SyncMetrics` + watch channel** — Already computes queue_depth, in_flight, failed_count per tick. Phase 2 exposes this to AppState, doesn't reinvent it.
- **`VfsEvent` enum + mpsc channel** — Error event types already defined. Phase 2 widens to broadcast (multi-consumer) and adds new event types.
- **`DiskCache::total_size()`** — Cache disk usage already queryable. Phase 2 adds entry_count and wraps in CacheManager::stats().
- **`WriteBackBuffer::list_pending()`** — Pending upload file list already available. Phase 2 surfaces it through the Tauri command.
- **`auth_degraded: AtomicBool`** — Auth state already tracked in AppState. Phase 2 includes it in get_dashboard_status response.
- **`DeltaSyncResult`** — changed_items and deleted_items already returned per sync cycle. Phase 2 feeds these into the activity buffer.
- **`notify.rs` error→notification mapping** — 20 notification functions define the error categories. Phase 2's actionable hints follow the same taxonomy.
- **Layered tracing registry** — `registry().with(filter).with(fmt).with(fmt)` at main.rs:431. Adding a custom error-capture Layer is a `.with()` call.

### Established Patterns
- **AppState + Mutex fields** — All runtime state lives in `AppState`, accessed via `app.state::<AppState>()`. New observability state (error buffer, activity buffer, event bus) follows this pattern.
- **MountCacheEntry tuple** — Per-mount data stored as `(CacheManager, InodeTable, DeltaSyncObserver, OfflineManager, offline_flag)`. SyncHandle needs to be added here or accessed through MountHandle.
- **Tauri command pattern** — `#[tauri::command] async fn name(app: AppHandle) -> Result<T, String>` with `.map_err(|e| e.to_string())`. All 25 commands follow this.
- **Frontend listen() + invoke()** — `window.__TAURI__.event.listen()` for push events, `window.__TAURI__.core.invoke()` for pull. Phase 2 adds new events and commands that Phase 3 will consume.

### Integration Points
- **`start_delta_sync()` loop** — After each successful `run_delta_sync()`, feed DeltaSyncResult into activity buffer and update last_synced timestamp. On error, feed into error accumulator.
- **`spawn_event_forwarder()`** (main.rs:1284) — Currently consumes VfsEvent for notifications only. Must also forward to error accumulator (or replace mpsc with broadcast).
- **`AppState` struct** — Add error accumulator, activity buffer, and event bus fields. Straightforward — it's a plain struct.
- **`invoke_handler!` registration** (main.rs:612) — Add new command functions to the existing handler list.
- **Tracing setup** (main.rs:431) — Add error-capture Layer as third `.with()` in the registry chain, if using the tracing Layer approach.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The data layer should be straightforward infrastructure that Phase 3's dashboard can consume without surprises.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 02-observability-infrastructure*
*Context gathered: 2026-03-18*
