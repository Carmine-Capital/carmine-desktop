# Phase 2: Observability Infrastructure - Research

**Researched:** 2026-03-18
**Domain:** Tauri v2 IPC, tokio event channels, ring buffers, Rust observability patterns for desktop VFS sync app
**Confidence:** HIGH

## Summary

Phase 2 builds the **data layer** that Phase 3's dashboard UI will consume. The work decomposes into four distinct areas: (1) new Tauri commands that aggregate per-drive sync state, errors, and cache stats from existing AppState/CacheManager data, (2) in-memory ring buffers (error accumulator + activity feed) populated from the delta sync loop and VFS event forwarder, (3) real-time event emission via Tauri `emit()` for state transitions and new errors, and (4) stat methods on CacheManager/DiskCache/PinStore that don't exist yet.

The critical architectural insight is that **most of the data already exists** -- `SyncMetrics` via watch channel, `DiskCache::total_size()`, `WriteBackBuffer::list_pending()`, `auth_degraded` AtomicBool, per-mount `offline_flag` AtomicBool. The work is primarily about (a) making this data reachable from Tauri commands (exposing `SyncHandle` to AppState), (b) adding a few missing stat methods (`DiskCache::entry_count()`, `CacheManager::stats()`), and (c) building the two ring buffers (100 errors, 500 activity entries) that aggregate from the delta sync loop and VFS event channel.

**Primary recommendation:** Keep the existing `mpsc::unbounded_channel<VfsEvent>` per mount as-is for the notification forwarder. Add a separate `tokio::sync::broadcast` channel in AppState for `ObsEvent`s that the event forwarder, delta sync loop, and mount lifecycle hooks all publish to. The broadcast channel feeds both the Tauri `emit()` bridge and the ring buffers. This avoids refactoring the VFS event_tx/event_rx plumbing while enabling multi-consumer observability.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Only persistent failures** reach the dashboard error log -- transient errors that auto-retry and succeed stay in log files only
- **Auth degradation is a state, not an error** -- surfaced via the degraded auth banner (DASH-04), not as an error log entry
- **Error sources: VFS events + delta sync errors** -- both VfsEvent types (ConflictDetected, WritebackFailed, UploadFailed, FileLocked) and delta sync failures (drive deleted, permission denied, item-level Graph failures) feed the accumulator
- **Include actionable hints per error type** -- each error entry carries a short action string
- **Individual entries per file** in activity feed -- a delta sync touching 150 files produces 150 activity entries
- **Tag each entry by type** -- "uploaded", "synced", "deleted", or "conflict"
- **Full remote path per entry** -- store the complete path from DriveItem
- **Files only** -- folder create/delete operations do not appear in the activity feed
- **Stale/Partial/Downloaded** pin health definitions with on-demand computation
- **Include file count breakdown** -- return `total_files` and `cached_files` per pin
- **Error buffer: 100 entries** -- ring buffer, oldest dropped when full
- **Activity buffer: 500 entries** -- ring buffer, oldest dropped when full
- **Return all, filter client-side** -- full buffer returned, Phase 3 filters in JS
- **In-memory only** -- buffers cleared on app restart

### Claude's Discretion
- Event bus implementation details (broadcast channel topology, subscriber management)
- Error accumulator internal data structures (ring buffer implementation, locking strategy)
- Cache stat method implementations on MemoryCache, DiskCache, CacheManager
- Tauri command naming conventions and response struct shapes (beyond what success criteria specify)
- Real-time event throttling/batching approach (how frequently emit() fires)
- Whether to use a custom tracing Layer for error capture or explicit error forwarding
- SyncHandle exposure strategy (stored in MountCacheEntry vs. separate channel)
- Per-mount sync state enum variants and transition logic

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.

</user_constraints>

## Standard Stack

### Core

No new crate dependencies. All capabilities come from existing workspace dependencies.

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio::sync::broadcast` | tokio 1.50 (workspace) | Multi-consumer event fanout for ObsEvent | Existing mpsc is single-consumer; broadcast lets error accumulator, activity buffer, notification forwarder, and Tauri emit bridge all subscribe independently |
| `tauri::Emitter` trait (`app.emit()`) | tauri 2 (workspace) | Push real-time events to frontend | Success criterion 4 specifies `emit()` + `listen()`. Already used for `auth-complete`, `refresh-settings` in existing code |
| `std::collections::VecDeque` | stdlib | Ring buffer backing for error + activity accumulators | Fixed-capacity ring buffer with O(1) push_back/pop_front. No allocation after initial capacity |
| `std::sync::Mutex` | stdlib | Guard for ring buffers in AppState | Consistent with existing AppState pattern (7+ Mutex fields). Short critical sections (push/drain) |
| `serde::Serialize` | serde 1.0 (workspace) | Serialize response structs for Tauri commands | All existing commands use this pattern |
| `chrono::Utc` | chrono 0.4 (workspace) | Timestamps for error and activity entries | Already used for DriveItem timestamps |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio::sync::watch` | tokio 1.50 (workspace) | SyncMetrics exposure (already used by SyncHandle) | Already exists -- just needs to be reachable from AppState |
| `dashmap` | 6.1 (workspace) | MemoryCache entry count via existing `.len()` | Already used for MemoryCache.entries -- no change needed |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio::sync::broadcast` for event bus | Replace `mpsc` with broadcast everywhere including VFS | Higher risk: requires refactoring CoreOps.event_tx and all call sites. Broadcast has Clone requirement. Keep mpsc for VFS, add broadcast for observability at app layer |
| `Mutex<VecDeque>` for ring buffers | `crossbeam::ArrayQueue` (lock-free) | Adds dependency. Mutex contention is negligible -- dashboard reads at ~1Hz, writes at event rate (tens/sec max). Lock-free complexity not justified |
| `app.emit()` for real-time events | `tauri::ipc::Channel` for streaming | Channel is faster but requires frontend to call `invoke('subscribe_dashboard', { onEvent })` and manage lifecycle. `emit()` is simpler, matches success criteria wording, and existing codebase pattern. Event volume (tens/sec max after throttling) is well within emit() capacity |
| Custom `tracing::Layer` for error capture | Explicit error forwarding from call sites | Layer approach captures ALL warn/error from any crate automatically but loses structured context (file name, drive_id). Explicit forwarding from known error sites preserves structured data. Recommend explicit forwarding since error sources are enumerated in CONTEXT.md |

**Installation:**
```bash
# No new packages -- all capabilities exist in workspace
```

## Architecture Patterns

### Recommended Project Structure Changes

```
crates/
  carminedesktop-core/src/
    types.rs             # ADD: ObsEvent enum (core so all crates can reference)
  carminedesktop-cache/src/
    manager.rs           # ADD: CacheManager::stats() method
    disk.rs              # ADD: DiskCache::entry_count() method
    pin_store.rs         # ADD: PinStore::health() method for on-demand pin health
  carminedesktop-app/src/
    main.rs              # MODIFY: AppState gains obs_tx, error_ring, activity_ring, last_synced map, sync_handles map
    commands.rs          # ADD: get_dashboard_status, get_recent_errors, get_cache_stats, get_activity_feed
    observability.rs     # NEW: ErrorAccumulator, ActivityBuffer, EventBridge, ObsEvent->emit forwarding
```

### Pattern 1: Dual-Mode Data Access (Pull Commands + Push Events)

**What:** Dashboard uses `invoke()` for initial load and periodic refresh of slowly-changing data (cache stats, mount states), and `listen()` for real-time activity/error events pushed via `emit()`.

**When to use:** Always. This is the Phase 2 architecture.

**Why:** Cache stats change on eviction runs (infrequent). Sync state changes per delta cycle (every 60s). But file activity and errors happen in real-time during user operations. Push for the real-time data, pull for the slow data.

**Pattern:**
```rust
// Pull: Tauri command returns snapshot
#[tauri::command]
async fn get_dashboard_status(app: AppHandle) -> Result<DashboardStatus, String> {
    let state = app.state::<AppState>();
    // Lock-per-field, never hold two locks simultaneously
    let authenticated = state.authenticated.load(Ordering::Relaxed);
    let auth_degraded = state.auth_degraded.load(Ordering::Relaxed);
    // ... build response from individual field reads
}

// Push: event bridge forwards ObsEvent to frontend
fn emit_obs_event(app: &AppHandle, event: &ObsEvent) {
    let _ = app.emit("obs-event", event);
}
```

```javascript
// Frontend: initial load + live updates
const status = await invoke('get_dashboard_status');
renderDashboard(status);
await listen('obs-event', (event) => {
    applyRealtimeUpdate(event.payload);
});
```

### Pattern 2: Event Bridge (Single Funnel)

**What:** A single async task (`EventBridge`) subscribes to the `broadcast::Receiver<ObsEvent>` and:
1. Emits each event to Tauri via `app.emit("obs-event", &event)`
2. Pushes error events into the ErrorAccumulator ring buffer
3. Pushes activity events into the ActivityBuffer ring buffer
4. Continues forwarding VFS error events to OS notifications (preserving existing behavior)

**When to use:** Spawned once at app startup (after `AppState` is initialized).

**Why:** Centralizes all event routing. The delta sync loop, VFS event forwarder, and mount lifecycle hooks all just `obs_tx.send(event)` -- they don't need to know about the dashboard, ring buffers, or notifications.

```rust
// In main.rs, after AppState is created:
let (obs_tx, _) = tokio::sync::broadcast::channel::<ObsEvent>(256);
// Store obs_tx in AppState, spawn bridge with obs_tx.subscribe()

async fn event_bridge(
    mut obs_rx: broadcast::Receiver<ObsEvent>,
    app: AppHandle,
    errors: Arc<Mutex<ErrorAccumulator>>,
    activity: Arc<Mutex<ActivityBuffer>>,
) {
    while let Ok(event) = obs_rx.recv().await {
        // 1. Emit to frontend
        let _ = app.emit("obs-event", &event);
        // 2. Route to ring buffers
        match &event {
            ObsEvent::Error { .. } => errors.lock().unwrap().push(event.clone()),
            ObsEvent::ActivityEntry { .. } => activity.lock().unwrap().push(event.clone()),
            _ => {}
        }
    }
}
```

### Pattern 3: SyncHandle Exposure via MountCacheEntry

**What:** Currently `SyncHandle` is passed into `MountHandle`/`CoreOps` at mount time and not stored in `MountCacheEntry`. To expose `SyncMetrics` to dashboard commands, store a clone of `SyncHandle` in `MountCacheEntry`.

**Current `MountCacheEntry`:**
```rust
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn DeltaSyncObserver>>,
    Arc<OfflineManager>,
    Arc<AtomicBool>,  // offline_flag
);
```

**New `MountCacheEntry` (add SyncHandle):**
```rust
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn DeltaSyncObserver>>,
    Arc<OfflineManager>,
    Arc<AtomicBool>,  // offline_flag
    Option<SyncHandle>,  // NEW: for SyncMetrics access
);
```

**Why:** `SyncHandle` is `Clone` (it wraps `mpsc::UnboundedSender` + `watch::Receiver`). Cloning it is cheap. The dashboard command reads `sync_handle.metrics()` which returns the latest `SyncMetrics` snapshot from the watch channel without any locking.

**Alternative considered:** Add a `MountHandle::sync_metrics()` accessor and read through the mounts HashMap. Rejected because `MountHandle` is platform-specific (`#[cfg]` gated) and `mount_caches` is already the uniform per-mount data store.

### Pattern 4: Ring Buffer with Fixed Capacity

**What:** Error and activity accumulators use `VecDeque` with manual capacity enforcement.

```rust
pub struct ErrorAccumulator {
    entries: VecDeque<DashboardError>,
    capacity: usize,
}

impl ErrorAccumulator {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, entry: DashboardError) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front(); // drop oldest
        }
        self.entries.push_back(entry);
    }

    pub fn drain(&self) -> Vec<DashboardError> {
        self.entries.iter().cloned().collect()
    }
}
```

**Why:** Simple, allocation-free after initialization (VecDeque capacity is pre-allocated). The 100-entry error buffer and 500-entry activity buffer are small enough that cloning the entire buffer for a Tauri command response is negligible (~50KB worst case for activity).

### Anti-Patterns to Avoid

- **Holding multiple AppState Mutex locks simultaneously**: The delta sync loop already locks `mount_caches` + `effective_config` together (lines 1525-1526 of main.rs). Dashboard commands MUST NOT do the same. Lock one field, extract data, release, then lock the next. Document lock ordering.
- **Emitting per-VFS-operation events**: Do NOT emit events for every `read`, `write`, `lookup`, `getattr`. These fire thousands of times per second. Only emit for meaningful lifecycle events (upload complete, sync complete, error).
- **Using bounded broadcast channel that's too small**: If the broadcast buffer fills and a slow receiver hasn't consumed, new sends succeed but slow receivers get `RecvError::Lagged`. Use 256 capacity (generous for the event rates) and handle `Lagged` by logging a warning and continuing.
- **Blocking Tauri command handlers on async cache operations**: `WriteBackBuffer::list_pending()` is async (reads filesystem). The Tauri command is already async, so this is fine, but do NOT call it inside a sync Mutex lock scope.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Multi-consumer event distribution | Custom subscriber list with `Arc<Mutex<Vec<Sender>>>` | `tokio::sync::broadcast` | Handles slow consumers, backpressure, subscriber lifecycle automatically |
| Per-drive sync metrics | Custom atomic counters in AppState | Existing `SyncHandle::metrics()` via `watch` channel | Already computed per tick by SyncProcessor. Just expose the handle |
| Disk cache size tracking | Walk filesystem to count bytes | Existing `DiskCache::total_size()` (SQLite query) | Already implemented and accurate |
| Pending upload enumeration | Query filesystem for pending files | Existing `WriteBackBuffer::list_pending()` | Already implemented, handles .tmp filtering |
| Timestamp formatting | Manual string formatting | `chrono::Utc::now().to_rfc3339()` | Already in workspace, consistent with DriveItem timestamps |
| Online/offline state | Custom connectivity checker | Existing `offline_flag: Arc<AtomicBool>` per mount | Already tracked and updated by delta sync loop and VFS timeout logic |

**Key insight:** ~70% of the data the dashboard needs already exists in the codebase. Phase 2 is primarily about (1) making existing data reachable from Tauri commands, (2) adding ring buffers for the two data types that don't have a natural home yet (errors and activity), and (3) emitting events at the right lifecycle points.

## Common Pitfalls

### Pitfall 1: Lock Contention Between Dashboard Commands and Delta Sync Loop

**What goes wrong:** The delta sync loop (main.rs:1525-1526) locks `mount_caches` and `effective_config` simultaneously. If a dashboard command does the same to gather mount status + config, it blocks for the entire delta sync duration.

**Why it happens:** Delta sync processes items inside the lock scope. With 5+ drives, lock hold time increases.

**How to avoid:** Dashboard commands must use snapshot-then-release: lock `mount_caches`, clone needed data into local variables, release lock. Never hold two AppState mutexes simultaneously in dashboard command handlers. Use atomics (`AtomicBool`, `AtomicU64`) for frequently-read values.

**Warning signs:** Dashboard polls occasionally take >100ms. UI status indicator stutters.

### Pitfall 2: Event Flooding from Delta Sync Producing 150+ Activity Entries

**What goes wrong:** A delta sync returning 150 changed items produces 150 `ObsEvent::ActivityEntry` events in rapid succession. Each triggers `app.emit()`, serializing JSON and crossing the IPC bridge.

**Why it happens:** User decision mandates individual entries per file.

**How to avoid:** Batch-emit activity entries. After processing a delta sync result, collect all activity entries into a single `Vec<ActivityEntry>` and emit once as `app.emit("activity-batch", &entries)`. The frontend handles the batch as a single DOM update. Also applies to VFS events during bulk operations.

**Warning signs:** WebView becomes sluggish during large syncs. `obs-event` listener fires 100+ times in under a second.

### Pitfall 3: SyncHandle Not Reachable from Tauri Commands

**What goes wrong:** `SyncHandle` is created in `start_mount()`, passed to `MountHandle`/`CoreOps`, but never stored in `MountCacheEntry`. Dashboard's `get_dashboard_status` cannot read `SyncMetrics`.

**Why it happens:** The original design didn't anticipate dashboard needing SyncMetrics. The handle is consumed by the VFS layer.

**How to avoid:** Clone `SyncHandle` before passing to `MountHandle` and store the clone in `MountCacheEntry`. `SyncHandle::Clone` is cheap (sender + watch receiver clones).

**Warning signs:** `get_dashboard_status` returns `queue_depth: 0, in_flight: 0` even during active uploads.

### Pitfall 4: Pin Health Computation Doing Graph API Calls

**What goes wrong:** The "stale" pin health check requires comparing `pinned_at` against the server's `last_modified` for items in the pinned tree. If implemented naively, this triggers Graph API calls for each pinned folder during `get_cache_stats`.

**Why it happens:** CONTEXT.md defines "stale = server content has changed since pin was last synced." Checking server state requires network.

**How to avoid:** Use the delta sync result to update staleness. During each delta sync, if any `changed_items` have a parent in the pinned set, mark that pin as stale (set a flag in the pin record or a separate in-memory map). The `get_cache_stats` command reads the flag without network calls. "Partial" and "Downloaded" can be computed entirely from local SQLite + disk cache data.

**Warning signs:** `get_cache_stats` takes >1s when pins exist. Network requests appear in logs during cache stat queries.

### Pitfall 5: `list_pending()` Is Async but Ring Buffer Lock Is Sync

**What goes wrong:** `WriteBackBuffer::list_pending()` returns a `Future` (it reads filesystem). If called inside a `Mutex` lock guard scope (e.g., while building a dashboard response holding `mount_caches` lock), the async call cannot proceed because the Mutex guard isn't `Send`.

**Why it happens:** `std::sync::Mutex` guard is `!Send`. Holding it across an `.await` point causes compilation errors.

**How to avoid:** Clone the `Arc<CacheManager>` from `mount_caches`, release the lock, then call `list_pending().await` on the clone. This is the standard snapshot-then-release pattern.

**Warning signs:** Compiler error: "future is not `Send`" or "MutexGuard cannot be held across await".

## Code Examples

### Example 1: ObsEvent Enum (in carminedesktop-core/src/types.rs)

```rust
// Source: designed based on CONTEXT.md decisions + existing VfsEvent pattern
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObsEvent {
    /// An error that should be displayed in the dashboard error log.
    Error {
        drive_id: Option<String>,
        file_name: Option<String>,
        error_type: String,
        message: String,
        action_hint: Option<String>,
        timestamp: String,
    },
    /// A file activity entry for the activity feed.
    Activity {
        drive_id: String,
        file_path: String,      // full remote path from DriveItem
        activity_type: String,   // "uploaded", "synced", "deleted", "conflict"
        timestamp: String,
    },
    /// Sync state transition for a drive.
    SyncStateChanged {
        drive_id: String,
        state: String,  // "syncing", "up_to_date", "error", "offline"
    },
    /// Online/offline state change.
    OnlineStateChanged {
        drive_id: String,
        online: bool,
    },
    /// Auth degradation state change.
    AuthStateChanged {
        degraded: bool,
    },
}
```

### Example 2: DashboardStatus Response Struct

```rust
// Source: derived from success criteria 1 in CONTEXT.md
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStatus {
    pub drives: Vec<DriveStatus>,
    pub authenticated: bool,
    pub auth_degraded: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatus {
    pub drive_id: String,
    pub name: String,
    pub mount_point: String,
    pub online: bool,
    pub last_synced: Option<String>,  // ISO 8601 or null
    pub sync_state: String,           // "up_to_date", "syncing", "error"
    pub upload_queue: UploadQueueInfo,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadQueueInfo {
    pub queue_depth: usize,
    pub in_flight: usize,
    pub failed_count: usize,
    pub total_uploaded: u64,
    pub total_failed: u64,
}
```

### Example 3: get_cache_stats Tauri Command

```rust
// Source: success criterion 3 in CONTEXT.md
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub disk_used_bytes: u64,
    pub disk_max_bytes: u64,
    pub memory_entry_count: usize,
    pub pinned_items: Vec<PinHealthInfo>,
    pub writeback_queue: Vec<WritebackEntry>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinHealthInfo {
    pub drive_id: String,
    pub item_id: String,
    pub folder_name: String,
    pub status: String,         // "downloaded", "partial", "stale"
    pub total_files: usize,
    pub cached_files: usize,
    pub pinned_at: String,
    pub expires_at: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritebackEntry {
    pub drive_id: String,
    pub item_id: String,
    pub file_name: Option<String>,  // resolved from SQLite if possible
}
```

### Example 4: Tauri emit() Pattern for Real-Time Events

```rust
// Source: existing pattern from commands.rs:105, tray.rs:141
use tauri::{AppHandle, Emitter};

fn emit_obs_event(app: &AppHandle, event: &ObsEvent) {
    if let Err(e) = app.emit("obs-event", event) {
        tracing::debug!("failed to emit obs event: {e}");
    }
}

// Batch variant for delta sync results
fn emit_activity_batch(app: &AppHandle, entries: &[ObsEvent]) {
    if let Err(e) = app.emit("activity-batch", entries) {
        tracing::debug!("failed to emit activity batch: {e}");
    }
}
```

```javascript
// Frontend pattern (Phase 3 will implement, Phase 2 verifies from browser console)
// Source: existing pattern from settings.js:510, wizard.js:142
const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;

// Verify success criterion 4:
await listen('obs-event', (event) => {
    console.log('obs-event:', event.payload);
});
```

### Example 5: CacheManager::stats() Method

```rust
// Source: new method on existing CacheManager (manager.rs)
impl CacheManager {
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            memory_entry_count: self.memory.len(),
            disk_used_bytes: self.disk.total_size(),
            disk_max_bytes: self.disk.max_size_bytes(),
            dirty_inode_count: self.dirty_inodes.len(),
        }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single-consumer mpsc for VfsEvent | Keep mpsc + add broadcast for ObsEvent | Phase 2 | VFS internals unchanged, observability layer is additive |
| No error buffering (log files only) | In-memory ring buffer (100 entries) | Phase 2 | Dashboard can show recent errors without parsing logs |
| No activity tracking | In-memory ring buffer (500 entries) | Phase 2 | Dashboard can show file-level activity feed |
| SyncMetrics only accessible from CoreOps | SyncHandle stored in MountCacheEntry | Phase 2 | Dashboard reads upload queue state via watch channel |
| No per-drive last_synced timestamp | `HashMap<String, Instant>` in AppState | Phase 2 | Dashboard shows "Last synced: 2 min ago" per drive |
| Pin health unknown | On-demand computation from SQLite + disk cache | Phase 2 | Dashboard shows downloaded/partial/stale per pin |

**Deprecated/outdated:**
- Prior research suggested `tauri::ipc::Channel` for streaming. The success criteria explicitly specify `emit()` + `listen()`, which is simpler and sufficient for the expected event volume. Channel is better for high-throughput streaming but adds lifecycle complexity (frontend must invoke a subscribe command).

## Open Questions

1. **DiskCache::max_size_bytes accessor**
   - What we know: `max_size_bytes` is an `AtomicU64` field on `DiskCache`. `total_size()` exists but no public `max_size_bytes()` accessor.
   - What's unclear: Whether to add a simple accessor or read from config.
   - Recommendation: Add `pub fn max_size_bytes(&self) -> u64` accessor on DiskCache. Simpler than re-parsing config.

2. **MemoryCache::len() accessor**
   - What we know: `MemoryCache.entries` is `DashMap` with a `.len()` method, but `entries` is private.
   - What's unclear: Whether to make it public or add a `len()` method.
   - Recommendation: Add `pub fn len(&self) -> usize { self.entries.len() }` to MemoryCache.

3. **Pin health "stale" detection without Graph API calls**
   - What we know: CONTEXT.md says "stale = server content changed since pin was last synced." Delta sync already returns `changed_items` with parent references.
   - What's unclear: Exact data flow for marking a pin stale when delta sync finds changed items in a pinned subtree.
   - Recommendation: During delta sync result processing in `start_delta_sync()`, check if any changed item has a parent_id matching a pinned folder. If so, update an in-memory `stale_pins: HashSet<(String, String)>` in AppState. The `get_cache_stats` command reads this set.

4. **Activity entry file_path resolution**
   - What we know: CONTEXT.md requires "full remote path per entry." `DriveItem.parent_reference.path` contains the parent path (e.g., `/drives/{id}/root:/Documents/Reports`). `DeletedItemInfo` has `parent_path: Option<String>`.
   - What's unclear: Whether `parent_reference.path` is always populated in delta sync results. The Graph API delta endpoint sometimes omits it.
   - Recommendation: Construct path from `parent_reference.path` + `name` when available. Fall back to name-only if path is missing. This is a data quality issue, not an architecture issue.

5. **Lock ordering documentation**
   - What we know: STATE.md gotcha notes "56 occurrences of `.lock().unwrap()` -- lock ordering must be documented during Phase 2." Delta sync currently locks `mount_caches` then `effective_config` (lines 1525-1526).
   - What's unclear: Full lock dependency graph across all code paths.
   - Recommendation: Document canonical order in AppState: `user_config` > `effective_config` > `mount_caches` > `mounts` > `sync_cancel` > `active_sign_in` > `account_id` > `obs_errors` > `obs_activity`. Dashboard commands must follow this order.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust integration tests (`#[tokio::test]`) |
| Config file | none -- test convention is `crates/<name>/tests/*.rs` |
| Quick run command | `toolbox run -c carminedesktop-build cargo test --all-targets -p carminedesktop-cache -p carminedesktop-app` |
| Full suite command | `toolbox run -c carminedesktop-build cargo test --all-targets` |

### Phase Requirements to Test Map

Phase 2 has no formal requirement IDs but has 4 success criteria. Each maps to testable behavior:

| Criterion | Behavior | Test Type | Automated Command | File Exists? |
|-----------|----------|-----------|-------------------|-------------|
| SC-1 | `get_dashboard_status` returns per-drive sync state, online/offline, last synced, auth health | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_get_dashboard_status` | No -- Wave 0 |
| SC-2 | `get_recent_errors` returns errors with file name, type, timestamp | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_get_recent_errors` | No -- Wave 0 |
| SC-3 | `get_cache_stats` returns disk usage, pin count, writeback queue | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_cache_stats` | No -- Wave 0 |
| SC-4 | Real-time events pushed via `emit()` | manual-only | Manual: subscribe with `listen()` in browser console during delta sync | N/A (requires running Tauri app) |
| N/A | ErrorAccumulator ring buffer capacity and ordering | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_error_accumulator` | No -- Wave 0 |
| N/A | ActivityBuffer ring buffer capacity and ordering | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_activity_buffer` | No -- Wave 0 |
| N/A | CacheManager::stats() returns correct values | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_cache_manager_stats` | No -- Wave 0 |
| N/A | PinStore health computation (downloaded/partial/stale) | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_pin_health` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `toolbox run -c carminedesktop-build cargo test --all-targets -p carminedesktop-cache -p carminedesktop-app`
- **Per wave merge:** `toolbox run -c carminedesktop-build cargo test --all-targets` + `toolbox run -c carminedesktop-build cargo clippy --all-targets --all-features`
- **Phase gate:** Full CI suite green (fmt + clippy + build + test) before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/carminedesktop-app/tests/observability_tests.rs` -- covers SC-1, SC-2, ring buffer tests. Note: testing Tauri commands requires either mocking AppState or testing the underlying data structures directly (ring buffers, stat methods). The Tauri command integration (actual IPC) is manual-only.
- [ ] `crates/carminedesktop-cache/tests/cache_stats_tests.rs` -- covers SC-3, CacheManager::stats(), PinStore::health()
- [ ] No framework install needed -- existing test infrastructure sufficient

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/carminedesktop-app/src/main.rs` (AppState struct, delta sync loop, event forwarder, mount lifecycle)
- Codebase analysis: `crates/carminedesktop-vfs/src/core_ops.rs` (VfsEvent enum, event_tx channel)
- Codebase analysis: `crates/carminedesktop-vfs/src/sync_processor.rs` (SyncMetrics, SyncHandle, watch channel)
- Codebase analysis: `crates/carminedesktop-cache/src/manager.rs` (CacheManager fields, no stats method)
- Codebase analysis: `crates/carminedesktop-cache/src/disk.rs` (DiskCache::total_size(), max_size_bytes AtomicU64)
- Codebase analysis: `crates/carminedesktop-cache/src/pin_store.rs` (PinStore, list_all(), is_protected())
- Codebase analysis: `crates/carminedesktop-cache/src/writeback.rs` (WriteBackBuffer::list_pending())
- [Tauri v2 -- Calling the Frontend from Rust](https://v2.tauri.app/develop/calling-frontend/) -- emit() and Channel API documentation
- Prior project research: `.planning/research/STACK.md`, `ARCHITECTURE.md`, `PITFALLS.md` (2026-03-18)

### Secondary (MEDIUM confidence)
- tokio::sync::broadcast documentation: https://docs.rs/tokio/latest/tokio/sync/broadcast/ -- standard module, verified in workspace version

### Tertiary (LOW confidence)
- None -- all findings verified against codebase or official documentation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- zero new dependencies, all capabilities verified in existing workspace crates
- Architecture: HIGH -- dual-mode (pull + push) pattern verified against existing codebase patterns (emit/listen in commands.rs, tray.rs, settings.js, wizard.js)
- Pitfalls: HIGH -- derived from direct codebase analysis (lock ordering, SyncHandle accessibility, async list_pending)
- Data model: HIGH -- response struct shapes derived directly from CONTEXT.md success criteria and existing type patterns

**Research date:** 2026-03-18
**Valid until:** 2026-04-18 (stable -- no external dependencies, all findings are codebase-specific)
