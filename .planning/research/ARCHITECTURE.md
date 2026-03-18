# Architecture Patterns

**Domain:** Desktop VFS sync observability & dashboard UI
**Researched:** 2026-03-18

## Recommended Architecture

### Overview: Event Bus + Pull Queries

The observability layer follows a **dual-mode architecture**: a push-based event bus for real-time activity (file operations, errors, state transitions) and pull-based Tauri commands for snapshot queries (cache stats, drive status, sync metrics). This mirrors what the codebase already partially implements — `VfsEvent` is a push channel, while `AppState` fields are queried via commands.

```
┌──────────────────────────────────────────────────────────────┐
│                    Frontend (Vanilla JS)                       │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────────┐   │
│  │ Dashboard UI │  │ Activity Feed│  │ Error Log Panel   │   │
│  └──────┬───┬──┘  └──────┬───────┘  └───────┬───────────┘   │
│         │   │            │                   │               │
│    invoke()  listen()    listen()        invoke() + listen() │
└─────────┼───┼────────────┼───────────────────┼───────────────┘
          │   │            │                   │
    ┌─────┼───┼────────────┼───────────────────┼───────────────┐
    │     ▼   ▼            ▼                   ▼  App Layer    │
    │  ┌──────────┐  ┌──────────────┐  ┌──────────────┐       │
    │  │ Commands  │  │ EventBridge  │  │ ErrorAccum   │       │
    │  │ (pull)    │  │ (push)       │  │ (ring buffer)│       │
    │  └──────┬───┘  └──────┬───────┘  └──────┬───────┘       │
    │         │              │                 │               │
    │         ▼              ▼                 ▼               │
    │  ┌─────────────────────────────────────────────┐        │
    │  │            AppState + ObsState              │        │
    │  │  (mount_caches, sync handles, error ring)   │        │
    │  └─────────────────────┬───────────────────────┘        │
    └────────────────────────┼────────────────────────────────┘
                             │
    ┌────────────────────────┼────────────────────────────────┐
    │                        ▼  VFS Layer                     │
    │  ┌──────────┐  ┌──────────────┐  ┌───────────────┐     │
    │  │ CoreOps   │  │SyncProcessor │  │ MountHandle   │     │
    │  │ VfsEvent  │  │ SyncMetrics  │  │ sync_handle   │     │
    │  │  (push)   │  │  (watch)     │  │               │     │
    │  └──────┬───┘  └──────┬───────┘  └───────────────┘     │
    └─────────┼──────────────┼────────────────────────────────┘
              │              │
    ┌─────────┼──────────────┼────────────────────────────────┐
    │         ▼              ▼  Cache Layer                    │
    │  ┌──────────┐  ┌──────────────┐  ┌───────────────┐     │
    │  │DiskCache  │  │WriteBackBuf  │  │MemoryCache    │     │
    │  │total_size │  │list_pending  │  │ entry count   │     │
    │  │           │  │              │  │               │     │
    │  └──────────┘  └──────────────┘  └───────────────┘     │
    └─────────────────────────────────────────────────────────┘
```

### Why Dual-Mode

**Push events (Tauri `emit()` → JS `listen()`)** for:
- File activity (upload started/completed/failed, download, conflict)
- State transitions (online→offline, auth degraded, mount started/stopped)
- Errors requiring user attention

**Pull queries (Tauri `invoke()` commands)** for:
- Dashboard snapshot on page load (cache sizes, mount states, sync status)
- Periodic refresh (every 5–10s) for slowly-changing stats
- User-initiated queries (error log history, pin list)

The push/pull split avoids the complexity of maintaining real-time state synchronization for data that changes infrequently (cache sizes) while giving immediate feedback for data that the user cares about seeing in real-time (file activity, errors).

### Component Boundaries

| Component | Crate | Responsibility | Communicates With |
|-----------|-------|----------------|-------------------|
| **ObsEvent enum** | `carminedesktop-core` | Unified observability event type (superset of current `VfsEvent`) | Emitted by VFS, cache, app layers; consumed by app EventBridge |
| **EventBridge** | `carminedesktop-app` | Forwards `ObsEvent`s to Tauri `emit()` and accumulates errors in ring buffer | Reads from MPSC channel, writes to Tauri webview + ErrorAccumulator |
| **ErrorAccumulator** | `carminedesktop-app` | Ring buffer of recent errors (capped ~200) for dashboard error log panel | Written by EventBridge, read by `get_recent_errors` command |
| **Dashboard commands** | `carminedesktop-app/commands.rs` | `get_dashboard_status`, `get_cache_stats`, `get_recent_errors`, `get_sync_metrics` | Reads AppState, CacheManager, SyncHandle, ErrorAccumulator |
| **SyncMetrics (existing)** | `carminedesktop-vfs` | Upload queue depth, in-flight count, total uploaded/failed (already built) | Exposed via `SyncHandle::metrics()` watch channel |
| **CacheManager stats** | `carminedesktop-cache` | Disk cache total_size (existing), memory entry count (new), writeback pending count (new) | Queried by dashboard commands |
| **Dashboard UI** | `dist/dashboard.html` + `dashboard.js` | Renders sync status, activity feed, errors, cache stats, offline pins | Calls invoke() for snapshots, listen() for live events |

### Data Flow

**Event flow (push path):**

```
VfsEvent emitted in CoreOps/SyncProcessor
    ↓ (unbounded MPSC — already exists)
spawn_event_forwarder in app crate
    ↓ (currently: notify::* only)
    ↓ (NEW: also emit to Tauri webview + error accumulator)
Tauri emit("obs-event", payload)
    ↓
Frontend listen("obs-event") → update activity feed + error panel
```

**Snapshot query (pull path):**

```
Frontend: invoke("get_dashboard_status")
    ↓
commands.rs: reads AppState.mount_caches, .mounts, .authenticated
    ↓ for each mount:
    ├── CacheManager.disk.total_size() — disk cache bytes
    ├── CacheManager.disk.max_size_bytes() — disk cache limit
    ├── CacheManager.writeback.list_pending() — pending upload count
    ├── MountHandle.sync_handle.metrics() — SyncMetrics snapshot
    ├── offline_flag.load() — online/offline state
    └── mount config — name, mount_point, enabled
    ↓
Returns: DashboardStatus { drives: Vec<DriveStatus> }
    ↓
Frontend: renders dashboard panels
```

**Delta sync observability (extending existing loop):**

The delta sync loop in `main.rs:start_delta_sync()` already processes results per drive. Extend it to:
1. Record `last_synced_at: Instant` per drive after successful sync
2. Emit `ObsEvent::DeltaSyncCompleted { drive_id, changed: usize, deleted: usize }` on success
3. Emit `ObsEvent::DeltaSyncFailed { drive_id, reason }` on error
4. These events flow through EventBridge → Tauri emit → dashboard updates "Last synced: Xs ago"

## Event Type Design

### Recommended: Extend VfsEvent into a Broader ObsEvent

The existing `VfsEvent` (in `carminedesktop-vfs/src/core_ops.rs`) has 4 variants — all error conditions. The observability system needs success events, state transitions, and cache events too.

**Option A (recommended): New `ObsEvent` in `carminedesktop-core`**

Define `ObsEvent` in core (like `DeltaSyncObserver` is) so all crates can emit events without circular dependencies. The existing `VfsEvent` stays as-is (it's VFS-internal), and the EventBridge in the app crate maps `VfsEvent` → `ObsEvent` variants.

```rust
// In carminedesktop-core/src/types.rs
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObsEvent {
    // Activity
    UploadStarted { drive_id: String, file_name: String },
    UploadCompleted { drive_id: String, file_name: String },
    UploadFailed { drive_id: String, file_name: String, reason: String },
    DownloadCompleted { drive_id: String, file_name: String },
    ConflictDetected { drive_id: String, file_name: String, conflict_name: String },
    FileLocked { drive_id: String, file_name: String },

    // Sync
    DeltaSyncCompleted { drive_id: String, changed: usize, deleted: usize },
    DeltaSyncFailed { drive_id: String, reason: String },

    // State transitions
    MountStarted { drive_id: String, name: String, mountpoint: String },
    MountStopped { drive_id: String, name: String },
    OnlineStateChanged { drive_id: String, online: bool },
    AuthDegraded,
    AuthRestored,

    // Errors (surfaced in error log)
    Error { drive_id: Option<String>, message: String, detail: Option<String> },
}
```

**Why this over extending VfsEvent:**
- `VfsEvent` is VFS-specific and lives in `carminedesktop-vfs`. Cache events and auth events can't emit VfsEvents without a dependency inversion.
- `ObsEvent` in core is available to all crates — same pattern as `DeltaSyncObserver`.
- The app-layer EventBridge is the single funnel that converts `VfsEvent` → `ObsEvent`, adds delta sync events, and adds auth state events.
- Frontend receives one event stream with one type, not multiple.

**Option B (simpler but less extensible): Keep VfsEvent, add more variants, emit from app layer only**

This works if observability events only originate from app-layer code (which wraps VFS and cache operations). But it conflates VFS-internal events with UI observability events, and the sync processor would need to emit through the existing MPSC channel (awkward for delta sync events that originate in app-layer code, not VFS code).

**Recommendation: Option A.** The cost is one new enum in core (~40 lines). The benefit is clean separation and extensibility.

### Frontend Event Contract

Events arrive as Tauri events with a tagged JSON payload:

```javascript
// In dashboard.js
listen("obs-event", (event) => {
  const { type, ...payload } = event.payload;
  switch (type) {
    case "uploadCompleted":
      addActivityEntry("upload", payload.fileName, "success");
      break;
    case "uploadFailed":
      addActivityEntry("upload", payload.fileName, "error");
      addError(payload);
      break;
    case "deltaSyncCompleted":
      updateDriveLastSynced(payload.driveId);
      break;
    case "onlineStateChanged":
      updateDriveOnlineState(payload.driveId, payload.online);
      break;
    // ...
  }
});
```

## Dashboard Data Model

### What the Frontend Needs (Pull)

```typescript
// Returned by get_dashboard_status command
interface DashboardStatus {
  drives: DriveStatus[];
  authenticated: boolean;
  authDegraded: boolean;
  appVersion: string;
}

interface DriveStatus {
  driveId: string;
  name: string;
  mountpoint: string;
  mounted: boolean;
  online: boolean;
  // Sync
  lastSyncedAt: string | null;     // ISO 8601 or null if never
  syncIntervalSecs: number;
  syncMetrics: SyncMetrics;        // from SyncHandle::metrics()
  // Cache
  diskCacheBytes: number;
  diskCacheMaxBytes: number;
  pendingUploadCount: number;
  // Offline
  pinnedFolders: PinnedFolder[];
}

interface SyncMetrics {
  queueDepth: number;
  inFlight: number;
  failedCount: number;
  totalUploaded: number;
  totalFailed: number;
  totalDeduplicated: number;
}

interface PinnedFolder {
  itemId: string;
  name: string;
  pinnedAt: string;
  expiresAt: string;
}

// Returned by get_recent_errors command
interface ErrorEntry {
  timestamp: string;
  driveId: string | null;
  message: string;
  detail: string | null;
}
```

### What the Frontend Needs (Push)

Real-time events for the activity feed — a scrolling list of recent file operations. The frontend keeps the last ~50 events in memory (no persistence needed). Each event becomes a row:

```
[timestamp] [icon] [fileName] — [status]
14:32:05    ↑     report.docx — Uploaded
14:32:01    ↓     budget.xlsx — Downloaded
14:31:55    ⚠     notes.txt   — Conflict (saved as notes.conflict.1741...)
14:31:30    ✓     Sync completed — 3 changed, 1 deleted
```

## Where Observability Events Should Originate

### Per Crate

| Crate | Event Source | What It Emits | How |
|-------|-------------|---------------|-----|
| **carminedesktop-core** | `ObsEvent` enum definition | (defines types only) | Enum definition + `ObsEventSender` trait |
| **carminedesktop-cache** | `DiskCache`, `WriteBackBuffer` | (passive — queried, not push) | Stats methods: `total_size()`, new `entry_count()`, `list_pending().len()` |
| **carminedesktop-vfs** | `CoreOps`, `SyncProcessor` | `VfsEvent` (existing): conflicts, upload failures, locks | Existing MPSC channel to app layer |
| **carminedesktop-vfs** | `SyncProcessor` | `SyncMetrics` (existing): queue depth, in-flight, totals | Existing `watch` channel via `SyncHandle::metrics()` |
| **carminedesktop-app** | `start_delta_sync()` loop | Delta sync completed/failed, online state changes | Direct: emits `ObsEvent` to EventBridge after each sync cycle |
| **carminedesktop-app** | `start_mount()` / `stop_mount()` | Mount started/stopped | Direct: emits after mount lifecycle |
| **carminedesktop-app** | Auth flow | Auth degraded/restored | Direct: emits on auth state transitions |
| **carminedesktop-app** | `EventBridge` | All `ObsEvent`s → Tauri `emit()` + ErrorAccumulator | Central funnel |

### Key Insight: App Layer is the Funnel

Most observability events don't need to originate in lower crates. The app layer already orchestrates all lifecycle operations (mount, sync, auth). Rather than threading an event sender through every crate, the app layer:

1. Wraps existing VfsEvent → ObsEvent conversion (already happening in `spawn_event_forwarder`)
2. Emits sync events directly after delta sync results (already in the sync loop)
3. Emits mount/auth events at lifecycle points (already in `start_mount`/`stop_mount`/auth flow)

The only new plumbing needed is:
- A `tokio::sync::broadcast` channel (or second unbounded MPSC) from EventBridge → Tauri emit
- An ErrorAccumulator (VecDeque ring buffer behind Mutex) for the error log

## Offline Pin Crash Investigation — Architectural Fit

The WinFsp offline pin crash (File Explorer hangs when navigating offline-pinned mount) is a VFS-layer bug. Architecturally, the investigation and fix belong in:

**Diagnosis path:**
1. **`carminedesktop-vfs/src/winfsp_fs.rs`** — WinFsp trait method implementations. The crash likely involves `ReadDirectory` or `GetFileInfo` blocking on Graph API calls when the mount is offline. The `offline_flag` AtomicBool is checked in `CoreOps` methods but the WinFsp backend may not be honoring it in all code paths.
2. **`carminedesktop-vfs/src/core_ops.rs`** — `list_children()` and `read_content()` should return from cache when offline, but edge cases (cache miss for unpopulated subdirectories, stale TTL) may cause Graph API calls that time out.
3. **`carminedesktop-cache/src/offline.rs`** — `OfflineManager::download_folder_recursive()` may not be downloading deeply nested children, leaving gaps in the cache that cause blocking fetches when Explorer enumerates.

**How observability helps the fix:**
- The `ObsEvent::OnlineStateChanged` and `ObsEvent::Error` events make offline transitions visible
- Adding `tracing::debug!` spans around WinFsp callbacks helps correlate which callback is hanging
- Dashboard shows online/offline state per drive — helps reproduce and verify the fix
- A new `ObsEvent::OfflineCacheMiss { drive_id, path }` event could expose exactly which path triggers the hang (useful during development, can be removed or gated later)

**Fix likely involves:**
- Ensuring `CoreOps::list_children()` returns `VfsError::NotFound` or empty list (not a network timeout) when offline + cache miss
- Making WinFsp `ReadDirectory` respond immediately with cached entries or STATUS_NO_MORE_FILES when offline
- Potentially pre-populating inode table entries during `OfflineManager::download_folder_recursive()`

**Build order implication:** The offline pin fix should come BEFORE dashboard observability, because:
1. It's a crash/hang bug — higher priority than new features
2. The investigation may reveal VFS response patterns that inform what observability events to emit
3. Observability infrastructure (event bus, error log) provides tools to validate the fix

## Patterns to Follow

### Pattern 1: EventBridge (extend existing spawn_event_forwarder)

The existing `spawn_event_forwarder` function converts VfsEvent → desktop notifications. Extend it to also emit Tauri events and accumulate errors.

**What:** Single async task that receives events from all sources and forwards to UI.
**When:** Always — started per mount, processes events for the mount's lifetime.
**Implementation sketch:**

```rust
// In carminedesktop-app/src/main.rs (refactored from spawn_event_forwarder)
fn spawn_event_bridge(
    rt: &tokio::runtime::Handle,
    app: &tauri::AppHandle,
    mut vfs_rx: UnboundedReceiver<VfsEvent>,
    error_log: Arc<ErrorAccumulator>,
    drive_id: String,
) {
    let app_handle = app.clone();
    rt.spawn(async move {
        while let Some(vfs_event) = vfs_rx.recv().await {
            // 1. Convert to ObsEvent
            let obs_event = ObsEvent::from_vfs_event(vfs_event, &drive_id);
            
            // 2. Desktop notification (existing behavior)
            obs_event.maybe_notify(&app_handle);
            
            // 3. Push to frontend
            let _ = app_handle.emit("obs-event", &obs_event);
            
            // 4. Accumulate errors
            if obs_event.is_error() {
                error_log.push(obs_event.into_error_entry());
            }
        }
    });
}
```

### Pattern 2: Snapshot Commands (new Tauri commands)

**What:** `#[tauri::command]` functions that collect state from AppState and return structured JSON.
**When:** Dashboard page load + periodic refresh (5–10s interval).
**Implementation sketch:**

```rust
#[tauri::command]
pub async fn get_dashboard_status(app: AppHandle) -> Result<DashboardStatus, String> {
    let state = app.state::<AppState>();
    let config = state.effective_config.lock().map_err(|e| e.to_string())?;
    let mounts = state.mounts.lock().map_err(|e| e.to_string())?;
    let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
    
    let mut drives = Vec::new();
    for mount_config in &config.mounts {
        let drive_id = mount_config.drive_id.as_deref().unwrap_or("");
        let mounted = mounts.contains_key(&mount_config.id);
        let (disk_size, pending_count, online, sync_metrics) = 
            if let Some((cache, _, _, _, offline_flag)) = caches.get(drive_id) {
                let disk = cache.disk.total_size();
                let pending = cache.writeback.list_pending().await
                    .map(|p| p.len()).unwrap_or(0);
                let online = !offline_flag.load(Ordering::Relaxed);
                let metrics = mounts.get(&mount_config.id)
                    .and_then(|h| h.sync_metrics());  // New accessor needed
                (disk, pending, online, metrics)
            } else {
                (0, 0, false, None)
            };
        // ... build DriveStatus
    }
    Ok(DashboardStatus { drives, ... })
}
```

### Pattern 3: ErrorAccumulator (ring buffer)

**What:** Thread-safe ring buffer of recent error events for the error log panel.
**When:** Errors arrive from EventBridge; queried by `get_recent_errors` command.
**Implementation sketch:**

```rust
pub struct ErrorAccumulator {
    entries: Mutex<VecDeque<ErrorEntry>>,
    max_entries: usize,
}

impl ErrorAccumulator {
    pub fn new(max_entries: usize) -> Self { ... }
    pub fn push(&self, entry: ErrorEntry) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(entry);
    }
    pub fn recent(&self, limit: usize) -> Vec<ErrorEntry> {
        self.entries.lock().unwrap().iter().rev().take(limit).cloned().collect()
    }
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}
```

### Pattern 4: Dashboard Page (new HTML/JS page)

**What:** `dashboard.html` + `dashboard.js` in `dist/`, opened from tray or as default authenticated view.
**When:** User clicks tray icon (replaces settings as default view, or becomes a tab/panel within it).
**Follows existing conventions:**
- Vanilla JS, no build step
- Uses `window.__TAURI__.core.invoke()` and `window.__TAURI__.event.listen()`
- Uses `showStatus()` from `ui.js` for feedback
- No inline event handlers (CSP `script-src 'self'`)
- `addEventListener` in JS only

**Key decision — separate page or panel within settings:**

**Recommendation: Dashboard as a new panel within `settings.html`.**

The settings page already has a panel navigation system (`state.activePanel`, `renderNav()`). Adding a "Dashboard" panel (as the first/default panel for authenticated users) is simpler than creating a new HTML page and avoids another window. The tray click handler opens settings → dashboard is the first panel.

This means:
- No new HTML file needed
- Dashboard JS added to `settings.js` (or extracted to `dashboard.js` and loaded via `<script src="dashboard.js">` in settings.html)
- Nav items expanded: Dashboard | Mounts | General | Advanced
- Dashboard becomes the default panel (`activePanel: 'dashboard'`)

## Anti-Patterns to Avoid

### Anti-Pattern 1: Real-Time Polling for Everything
**What:** Using `setInterval` to poll all dashboard data at 1s intervals.
**Why bad:** Wastes CPU/battery on a desktop app. Cache stats change slowly. Polling SyncMetrics at 1s when the tick interval is already 1s creates unnecessary IPC overhead.
**Instead:** Push events for activity/errors. Poll snapshot every 5–10s. Use Tauri events for state transitions (online/offline).

### Anti-Pattern 2: Threading Event Senders Through All Crates
**What:** Adding `ObsEvent` sender parameters to CacheManager, SqliteStore, DiskCache methods.
**Why bad:** Couples observability to business logic. Every cache operation would need to construct and send events. Cache layer should remain a pure data layer.
**Instead:** The app layer observes outcomes and emits events. Cache provides query methods for stats.

### Anti-Pattern 3: Persisting Activity Feed
**What:** Storing the activity feed (upload/download events) in SQLite or files.
**Why bad:** This is ephemeral UI state, not application state. Adds write amplification on every file operation. No user expects to see activity from previous sessions.
**Instead:** In-memory VecDeque in ErrorAccumulator (errors only, capped). Activity feed is purely frontend-side (last ~50 events in JS array, lost on page navigation — acceptable).

### Anti-Pattern 4: Separate Observability Runtime
**What:** Creating a dedicated Tokio runtime or thread pool for observability.
**Why bad:** CarmineDesktop already runs on one Tokio runtime. Observability is lightweight (emit serialized JSON). A separate runtime adds complexity and potential for deadlocks.
**Instead:** Use the existing Tokio runtime. EventBridge is one spawned task per mount. Tauri commands run on the async runtime normally.

### Anti-Pattern 5: Breaking the Dependency Graph
**What:** Making `carminedesktop-vfs` depend on `carminedesktop-app` or Tauri types.
**Why bad:** Circular dependency. VFS must remain independent of the app layer.
**Instead:** `ObsEvent` in core (available to all). VFS emits `VfsEvent` through existing MPSC channel. App layer converts and forwards.

## New Methods Needed on Existing Types

The dashboard commands need a few stat methods that don't exist yet:

| Type | New Method | Returns | Purpose |
|------|-----------|---------|---------|
| `MemoryCache` | `entry_count() -> usize` | `self.entries.len()` | Dashboard cache panel |
| `WriteBackBuffer` | `pending_count() -> usize` | `self.buffers.len()` + disk count | Dashboard pending uploads |
| `MountHandle` (FUSE) | `sync_metrics() -> Option<SyncMetrics>` | `self.sync_handle.as_ref().map(\|h\| h.metrics())` | Dashboard sync panel |
| `WinFspMountHandle` | `sync_metrics() -> Option<SyncMetrics>` | Same pattern | Dashboard sync panel |
| `SqliteStore` | `item_count() -> usize` | `SELECT COUNT(*) FROM items` | Dashboard cache panel |
| `PinStore` | `list_pins(drive_id) -> Vec<PinRecord>` | Pin records with name/dates | Dashboard offline panel |

All are trivial getters — no architectural changes, no new dependencies.

## Build Order (Dependencies Between Observability Components)

```
Phase order (each depends on the one above):

1. Offline pin crash fix
   ├── Investigate WinFsp offline behavior
   ├── Fix VFS responses during offline state
   └── No dependency on observability infra

2. ObsEvent enum + ErrorAccumulator
   ├── Define ObsEvent in carminedesktop-core
   ├── Build ErrorAccumulator in carminedesktop-app
   ├── Add stat methods to cache types (entry_count, etc.)
   └── Foundation for everything below

3. EventBridge (extend spawn_event_forwarder)
   ├── Convert VfsEvent → ObsEvent
   ├── Emit to Tauri webview
   ├── Accumulate errors
   └── Depends on: ObsEvent enum, ErrorAccumulator

4. Dashboard commands (pull-based)
   ├── get_dashboard_status
   ├── get_cache_stats
   ├── get_recent_errors
   ├── get_sync_metrics per drive
   └── Depends on: ErrorAccumulator, stat methods

5. Dashboard UI
   ├── Dashboard panel in settings.html
   ├── Activity feed (listens to obs-event)
   ├── Error log panel
   ├── Drive status cards
   ├── Cache usage display
   └── Depends on: EventBridge, dashboard commands

6. Delta sync observability
   ├── Emit ObsEvent from start_delta_sync loop
   ├── Track last_synced_at per drive
   ├── Show "Last synced: Xs ago" in dashboard
   └── Depends on: EventBridge, Dashboard UI

7. UI polish
   ├── Visual refinement
   ├── Responsive status indicators
   └── Depends on: Everything above working
```

### Why This Order

1. **Offline pin fix first** — it's a crash bug blocking deployment. No feature dependency.
2. **ObsEvent + ErrorAccumulator** are the foundation — cheap to build (~100 lines each), everything else depends on them.
3. **EventBridge before commands** — extending the existing forwarder is a small refactor, and it immediately makes errors visible even before the dashboard exists (via desktop notifications, which already work).
4. **Commands before UI** — the data layer should be queryable before building the view. Allows testing with `invoke()` from browser console.
5. **Dashboard UI** — renders data from steps 3+4. Can be built incrementally (drive status first, then activity feed, then error log).
6. **Delta sync observability** — enhances dashboard with sync timing data. Not blocking for MVP dashboard.
7. **Polish last** — visual refinement after functionality is solid.

## Scalability Considerations

| Concern | Current Scale | At 5 Drives | Notes |
|---------|--------------|-------------|-------|
| Event volume | ~1-10 events/min | ~5-50 events/min | Negligible. Unbounded MPSC can handle thousands/sec. |
| ErrorAccumulator size | 200 entries max | Same | Ring buffer, constant memory |
| Dashboard polling | 1 invoke per refresh | Same (returns all drives) | Single command returns full snapshot |
| SyncMetrics watches | 1 per mount | 5 | Each is a tokio::sync::watch — near-zero cost |
| Memory for activity feed | ~50 events × ~200 bytes | Same | Frontend-only, ~10KB |

CarmineDesktop targets single-user desktops with 1–5 mounted drives. Scalability is not a concern for the observability layer.

## Sources

- Codebase analysis: `VfsEvent` enum in `crates/carminedesktop-vfs/src/core_ops.rs:344-358`
- Codebase analysis: `SyncMetrics` + `SyncHandle::metrics()` in `crates/carminedesktop-vfs/src/sync_processor.rs:56-96`
- Codebase analysis: `spawn_event_forwarder` in `crates/carminedesktop-app/src/main.rs:1276-1302`
- Codebase analysis: `DeltaSyncObserver` trait in `crates/carminedesktop-core/src/types.rs:4-16`
- Codebase analysis: Tauri `emit()` usage in `crates/carminedesktop-app/src/commands.rs:105-113`
- Codebase analysis: Frontend `listen()` pattern in `crates/carminedesktop-app/dist/settings.js:2,510`
- Codebase analysis: `DiskCache::total_size()` in `crates/carminedesktop-cache/src/disk.rs:252-263`
- Codebase analysis: `AppState` struct in `crates/carminedesktop-app/src/main.rs:235-255`
- Codebase analysis: `MountHandle` struct in `crates/carminedesktop-vfs/src/mount.rs:80-90`
- Codebase analysis: `start_delta_sync()` loop in `crates/carminedesktop-app/src/main.rs:1490-1639`
- Tauri v2 `Emitter` trait: used in app crate (`use tauri::{AppHandle, Emitter}`) — HIGH confidence
- Tauri v2 `listen()` JS API: used in `settings.js` and `wizard.js` — HIGH confidence
- `tokio::sync::watch` for metrics: used in `SyncProcessor` — HIGH confidence

---

*Architecture analysis: 2026-03-18*
