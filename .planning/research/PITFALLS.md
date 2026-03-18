# Domain Pitfalls

**Domain:** Desktop VFS sync app — stabilization & observability milestone
**Researched:** 2026-03-18
**Confidence:** HIGH (derived from codebase analysis + domain expertise in FUSE/WinFsp, Tauri IPC, and vanilla JS dashboard patterns)

## Critical Pitfalls

### Pitfall 1: WinFsp Offline Pin — Explorer Hangs on Blocking `resolve_path` Calls

**What goes wrong:** When a user navigates a pinned folder in File Explorer while the VFS is offline, Explorer's `get_security_by_name` and `open` callbacks call `CoreOps::resolve_path`, which walks each path component via `find_child`. `find_child` falls through memory → SQLite → Graph API. If memory cache has been evicted (10,000 entry cap, 60s TTL) and SQLite is missing children, the code calls `rt.block_on(graph.list_children(...))` even though the `is_offline()` check _should_ short-circuit this. The problem: `is_offline()` uses `Ordering::Relaxed`, and the offline flag may not be set yet if the _first_ network call in this path component walk is the one that fails. That first call blocks the WinFsp thread for the full HTTP timeout (~30s), and Explorer's UI thread hangs waiting on the WinFsp IPC response.

**Why it happens:**
- `find_child` only calls `set_offline()` after a Graph API call _fails_ with a `Network` error. The first call in a cold-cache path walk blocks for the full timeout.
- WinFsp's `get_security_by_name` is called by Explorer _synchronously_ for every item in the navigation pane. One 30-second block hangs the entire Explorer window.
- Memory cache TTL (60s) and max entries (10,000) mean pinned folder metadata can be evicted between user navigations.
- The pre-fetch at mount time only seeds root children — deeper pinned folder trees have cold caches after an eviction cycle.

**Consequences:**
- File Explorer becomes unresponsive (the infamous "Not Responding" state).
- Users force-kill Explorer, which may leave WinFsp mount in an inconsistent state.
- On Windows organizational deployments this is a showstopper — IT helpdesk tickets guaranteed.

**Prevention:**
1. **Pinned items must never be evicted from memory cache.** The `DiskCache` already has `is_protected()` parent-chain walk for eviction protection, but `MemoryCache::maybe_evict()` has no such check. Add eviction protection for inodes that are ancestors of pinned folders.
2. **Set a short HTTP timeout for VFS callbacks.** The Graph client's timeout should be aggressive (3-5s) for calls originating from `CoreOps` methods invoked by WinFsp/FUSE callbacks. Fall through to offline mode fast rather than blocking the FS thread pool.
3. **Pre-populate SQLite with full pinned folder tree.** When `pin_folder` downloads content recursively, ensure all metadata (not just content) is persisted to SQLite so that `find_child` never needs to hit the Graph API for pinned folders.
4. **Check offline flag _before_ each `find_child` network fallback**, not just inside `find_child` — or better, make `resolve_path` itself check the flag at each step and return a "stale cache" result rather than blocking.

**Detection:**
- Windows-specific testing: navigate a pinned folder with network cable unplugged.
- Monitor `find_child` Graph API fallback frequency via tracing — any call for a pinned folder's subtree indicates missing cache coverage.
- Watch for `list_children` calls in offline mode — these should never happen.

**Phase:** Must be fixed in the offline pin bug fix phase (before observability work begins). This is the deployment blocker.

---

### Pitfall 2: Event Flooding from Observability Instrumentation

**What goes wrong:** Adding event emission to the VFS hot path (every `read`, `write`, `lookup`, `getattr`) generates thousands of events per second during normal file operations. A single `ls -la` on a directory with 500 files triggers 500+ `lookup` + `getattr` calls. Opening a 100MB file triggers hundreds of `read` calls. The `UnboundedSender<VfsEvent>` channel grows without backpressure, consuming memory. If the dashboard UI tries to render each event, the Tauri WebView becomes unresponsive.

**Why it happens:**
- The current `VfsEvent` enum has only 4 variants (conflict, writeback failure, upload failure, file locked) — all are _rare_ events. Observability requires instrumenting _routine_ operations (reads, writes, lookups, dir listings).
- `tokio::sync::mpsc::UnboundedSender` has no backpressure — it's correct for rare events but will accumulate unbounded memory with high-frequency events.
- VFS callbacks (`read`, `write`, `getattr`) are called from FUSE/WinFsp thread pools via `rt.block_on()`. If event emission blocks (e.g., bounded channel full), it deadlocks the FUSE thread waiting on the Tokio runtime that's already occupied by VFS work.

**Consequences:**
- Memory growth proportional to file operation rate (can reach hundreds of MB during bulk operations like `git clone` or `npm install` inside the mount).
- Dashboard UI becomes sluggish trying to render thousands of DOM updates per second.
- Tauri IPC serialization overhead (JSON encoding each event) saturates the WebView bridge.

**Prevention:**
1. **Aggregate, don't stream individual events.** Use a `SyncMetrics`-style approach (already exists in `sync_processor.rs`) — maintain atomic counters in `CoreOps` (reads/sec, writes/sec, bytes transferred, lookups/sec, cache hits/misses). Expose via a periodic poll command, not per-event emission.
2. **Use `tokio::sync::watch` for metrics, not `mpsc` for events.** `watch` channels drop intermediate values — the dashboard always sees the latest snapshot. This is exactly how `SyncMetrics` already works with `metrics_rx: watch::Receiver<SyncMetrics>`.
3. **Rate-limit frontend updates to 1-2 Hz.** Even if the backend can update metrics 100x/sec, the dashboard should poll or receive updates at human-readable rates (500ms-1s intervals).
4. **Keep the existing `VfsEvent` channel for rare events only.** Don't expand it for observability — it's correctly designed for infrequent, important notifications.

**Detection:**
- Profile memory usage during bulk operations (copy 1000 files into the mount).
- Monitor `VfsEvent` channel depth — if it grows beyond ~100 pending events, the system is flooding.
- Benchmark Tauri `emit` overhead: serialize 1000 events/sec and measure WebView responsiveness.

**Phase:** Must be addressed when designing the observability architecture (before implementing the dashboard). Wrong choice here means a rewrite.

---

### Pitfall 3: Lock Contention When Adding Event Emission to Mutex-Heavy Code

**What goes wrong:** `AppState` has 7+ `Mutex` fields. Adding observability requires reading state from multiple mutexes to compose a dashboard snapshot (mount status, sync state, cache stats, offline flags). Naively locking all mutexes to build a snapshot in a Tauri command handler introduces contention with the delta sync loop (which already locks `mount_caches` + `effective_config` simultaneously, lines 1519-1520 of `main.rs`).

**Why it happens:**
- The delta sync loop runs every 60 seconds and holds `mount_caches` + `effective_config` locks while building the sync snapshot. If a dashboard poll command tries to lock the same mutexes at the same moment, it blocks until delta sync releases them.
- `SqliteStore` uses a single `Mutex<Connection>` — every metadata query serializes through it. Adding "get cache size" or "count pending uploads" queries to the dashboard polling path competes with VFS read/write operations.
- The 56 occurrences of `.lock().unwrap()` across the codebase mean any panic while holding a lock poisons the mutex, crashing all subsequent dashboard polls.

**Consequences:**
- Dashboard polls occasionally take 100ms+ instead of <5ms, causing visible UI jank (the status indicator "stutters").
- Worst case: deadlock if a new dashboard command locks mutexes in a different order than the delta sync loop.
- If any lock is poisoned (e.g., a panic in a VFS callback holding `SqliteStore`), the dashboard becomes permanently broken until restart.

**Prevention:**
1. **Use lock-free reads where possible.** `AtomicBool` (already used for `authenticated`, `auth_degraded`, offline flags), `AtomicU64` for counters (reads, writes, bytes), `AtomicUsize` for cache entry counts. These can be read by the dashboard without any mutex.
2. **Snapshot-then-release pattern.** When building a dashboard response, lock each mutex individually, extract the needed data into local variables, and release immediately — never hold two `AppState` mutexes simultaneously. The delta sync loop already _mostly_ does this but lines 1519-1520 are the exception.
3. **Document and enforce lock ordering.** Create a comment block in `AppState` defining the canonical order: `user_config` → `effective_config` → `mount_caches` → `mounts` → `sync_cancel` → `active_sign_in` → `account_id`. The delta sync violation (locks `mount_caches` before `effective_config`) should be fixed.
4. **Replace `SqliteStore` mutex with `tokio::sync::Mutex` or connection pool** for dashboard queries so they don't block VFS operations (or add a separate read-only connection for stats queries).

**Detection:**
- Add `tracing::debug!` around lock acquisitions with timing — any lock wait >5ms is a warning sign.
- Test dashboard responsiveness while running `find /mount -type f` (triggers heavy VFS + cache activity).
- Check for deadlocks: run dashboard poll + delta sync simultaneously under load.

**Phase:** Must be addressed during the observability architecture phase. The lock ordering documentation should happen during the main.rs refactor (if that precedes observability).

---

### Pitfall 4: Tauri IPC Overhead for Real-Time Dashboard Updates

**What goes wrong:** Each `invoke()` call from the frontend goes through Tauri's IPC bridge: JS → WebView → IPC channel → Rust handler → serialize response → IPC channel → WebView → JS. For polling-based dashboard updates at 1Hz with 3-4 data sources (sync status, cache stats, activity feed, error log), this means 3-4 round-trips per second. Each round-trip involves JSON serialization of the response payload. If the dashboard sends a single monolithic request that aggregates all data, the Rust handler must lock multiple mutexes to compose the response, re-introducing Pitfall 3.

**Why it happens:**
- Tauri v2 IPC is designed for request-response, not streaming. There's no built-in server-push mechanism except `app.emit()` which broadcasts to all windows.
- The current codebase uses `invoke()` for everything (settings, mounts, pins) — there's no existing pattern for push-based updates.
- JSON serialization of large payloads (e.g., a list of 500 recent file operations) adds measurable latency per poll.

**Consequences:**
- Dashboard feels laggy if polling interval is too slow (>2s), or consumes excessive CPU if polling too fast (<250ms).
- Multiple rapid `invoke()` calls can queue up if the Rust side is busy (e.g., during delta sync), causing stale data display.
- Large responses (activity log with hundreds of entries) cause GC pressure in the WebView.

**Prevention:**
1. **Use Tauri `emit()` for push-based updates.** The codebase already uses `emit("auth-complete", ...)` and `emit("refresh-settings", ...)`. Extend this pattern: emit `sync-status-update` from the delta sync loop, `activity-event` for notable operations, `cache-stats-update` after eviction runs.
2. **Emit lightweight payloads.** Send only changed/delta data, not full snapshots. For sync status: `{ drive_id, last_synced, items_changed, is_offline }` (~100 bytes). For activity: individual events with a capped buffer (keep last 100 in Rust, send newest on each emit).
3. **Use `invoke()` only for initial load and user-triggered actions.** Dashboard init calls `invoke("get_dashboard_state")` once for the full snapshot, then relies on `emit()` for incremental updates.
4. **Batch related data into a single emit.** Don't emit separate events for sync status, online/offline toggle, and error count — combine into one `dashboard-update` event.

**Detection:**
- Profile Tauri IPC latency: time each `invoke()` round-trip in the JS console.
- Monitor WebView memory usage during sustained dashboard operation (leave dashboard open for 1 hour).
- Check CPU usage with dashboard open vs. closed — should be <1% difference.

**Phase:** Dashboard implementation phase. The IPC strategy must be decided before writing dashboard JS, not retrofitted.

## Moderate Pitfalls

### Pitfall 5: Vanilla JS Dashboard Performance Without Virtual DOM

**What goes wrong:** The existing `settings.js` uses `innerHTML = ''` + `document.createElement()` loops to render lists (see `renderMounts()`, line 61). This pattern rebuilds the entire DOM subtree on every render. For a dashboard showing file activity (potentially hundreds of items), sync status per drive, and cache stats, calling `render()` on every state change triggers expensive layout recalculations.

**Why it happens:**
- `setState(patch)` calls `render()` which calls every `render*()` function, each of which rebuilds its DOM section from scratch. There's no diffing or selective update.
- The settings page has ~10 mounts maximum, so the cost is invisible. A dashboard with a scrolling activity feed of 100+ items will show jank.
- `list.innerHTML = ''` forces a full reflow when followed by `appendChild()` in a loop without `DocumentFragment`.

**Prevention:**
1. **Use `DocumentFragment` for batch DOM insertions.** Build all list items in a fragment, then append once. This eliminates intermediate reflows.
2. **Implement targeted updates instead of full re-renders.** For the activity feed: append new items to the top, remove items exceeding the cap from the bottom. For stats counters: update `textContent` on specific elements, not rebuild the section.
3. **Cap visible activity items at 50-100.** Older items scroll off. No need for virtual scrolling at this scale — `overflow-y: auto` on a fixed-height container is sufficient.
4. **Separate render functions per dashboard section** with dirty flags — only re-render sections whose data actually changed. The existing pattern of `renderNav()`, `renderSettings()`, `renderMounts()` as separate functions is good — extend it.
5. **Use CSS `content-visibility: auto`** for off-screen sections to skip rendering of non-visible panels.

**Detection:**
- Chrome DevTools Performance tab: check for >16ms frames during dashboard updates.
- Monitor paint count — should not exceed 2-3 paints per dashboard update cycle.
- Test with 200+ items in the activity feed and verify smooth scrolling.

**Phase:** Dashboard implementation phase.

---

### Pitfall 6: Memory Cache Eviction Breaks Offline Pin Invariants

**What goes wrong:** `MemoryCache::maybe_evict()` evicts the oldest entries regardless of whether they're part of a pinned folder tree. After eviction, the next `find_child` for a pinned item falls through to SQLite (usually fine) but then the freshly-looked-up item goes back into memory cache, evicting another old entry. Under heavy filesystem activity with a cold cache, pinned folders churn through the memory cache repeatedly — each lookup triggers SQLite I/O, which is 10-100x slower than the DashMap lookup, degrading performance.

**Why it happens:**
- `DiskCache` has `is_protected()` which walks parent chains to check pinned status — `MemoryCache` has no equivalent.
- The 10,000 entry cap is global across all drives. A single drive with many active files can push all other drives' pinned items out of memory.
- Memory cache TTL (60s) means even pinned items expire and need re-fetching from SQLite on next access.

**Prevention:**
1. **Exempt pinned inodes from memory cache eviction.** Before evicting, check if the inode is in the `PinStore`'s protected set. This mirrors the `DiskCache` protection strategy.
2. **Consider separate memory cache partitions per drive** so one active drive doesn't starve others.
3. **Extend TTL for pinned items** (or use `u64::MAX` TTL) in memory cache so they persist until explicitly invalidated by delta sync.

**Detection:**
- Log `MemoryCache` eviction events with the evicted inode — cross-reference with pinned folder inodes.
- Monitor SQLite query frequency for pinned item IDs — should be near zero after initial load.

**Phase:** Offline pin bug fix phase (closely related to Pitfall 1).

---

### Pitfall 7: Delta Sync Loop Competing with Dashboard for Lock Access

**What goes wrong:** The delta sync loop (lines 1516-1542 of `main.rs`) locks `mount_caches` and `effective_config` simultaneously to build its snapshot. If a dashboard Tauri command does the same to gather mount status + config for display, and the delta sync takes long (e.g., processing 1000 changed items), the dashboard poll blocks for the entire delta sync duration (potentially seconds).

**Why it happens:**
- Delta sync processes items inside the lock scope (building the snapshot Vec). After releasing the locks, it runs the actual sync operations. But the lock hold time includes iterating all mount_caches entries and all config mounts — O(n) in number of drives.
- A dashboard polling at 1Hz has a ~1.7% chance of colliding with a 1-second lock hold on a 60-second interval. With 5+ drives, lock hold time increases.

**Prevention:**
1. **Move dashboard-visible state to lock-free structures.** Maintain a `watch::Sender<DashboardState>` updated after each delta sync completes. Dashboard reads the `watch::Receiver` without locking anything.
2. **Minimize lock hold time in delta sync.** The current code already clones data out of the locks, which is good — verify no future changes hold locks longer.
3. **Never add "get dashboard state" as a Tauri command that locks AppState mutexes.** Instead, have the delta sync loop (and mount/unmount operations) push state updates to a shared `DashboardState`.

**Detection:**
- Time lock acquisition in Tauri command handlers — any wait >10ms is a problem.
- Run `invoke("get_dashboard_state")` in a tight loop while delta sync is active — measure p99 latency.

**Phase:** Observability architecture phase.

---

### Pitfall 8: `open_file` Metadata Refresh Blocking WinFsp Callbacks During Network Issues

**What goes wrong:** `CoreOps::open_file()` calls `rt.block_on(self.graph.get_item(...))` to refresh metadata from the server on every file open. In offline mode, this is skipped. But during _degraded_ network (high latency, packet loss — not full disconnection), each `open_file` blocks the WinFsp thread for the duration of the Graph API call (potentially 5-30 seconds with retries). Explorer waits for the `open` callback to return before showing file contents.

**Why it happens:**
- The metadata refresh exists to fix the `FUSE_WRITEBACK_CACHE` stale-size bug. It's necessary on Linux. On WinFsp, it may be less critical since WinFsp doesn't have the same kernel caching behavior.
- `set_offline()` is only triggered by `Network` error variants — a 500 server error or 503 with retry doesn't set offline mode, so the code retries the failing call.
- The retry logic in `with_retry()` uses exponential backoff, which can extend a single failed call to 30+ seconds.

**Prevention:**
1. **Use a separate, shorter timeout for VFS-callback-path Graph API calls.** The retry policy for `open_file` metadata refresh should have max 1 retry, 3-second timeout — not the default retry policy designed for upload reliability.
2. **Treat metadata refresh failures as non-fatal in `open_file`.** If the refresh fails, use cached metadata and log a warning. The file can still be opened — worst case the user sees a stale size that corrects on next read.
3. **Track degraded network state separately from offline.** After N consecutive slow responses (>2s), temporarily skip metadata refresh for 60 seconds and rely on cached data.

**Detection:**
- Monitor `open_file` latency distribution — p99 should be <100ms (cache hit). Any call taking >2s indicates a network issue.
- Test with a network throttler (e.g., `tc netem` on Linux) simulating 50% packet loss.

**Phase:** Offline pin bug fix phase (same root cause: VFS callbacks blocking on network during degraded connectivity).

## Minor Pitfalls

### Pitfall 9: Dashboard HTML/JS Growing Beyond Maintainable Size Without a Framework

**What goes wrong:** `settings.js` is already 520 lines for a relatively simple settings page. A dashboard with sync status, activity feed, error log, cache stats, and per-drive status will add 500-1000+ lines. Without component abstraction, the file becomes a monolith of `document.createElement()` calls, event listener wiring, and state management.

**Prevention:**
1. **Create `dashboard.js` as a separate file** — don't append dashboard logic to `settings.js`.
2. **Extract reusable UI patterns into `ui.js`** (which currently only has `showStatus` and `formatError`). Add: `createListItem()`, `createStatCard()`, `createActivityEntry()`, `formatBytes()`, `formatRelativeTime()`.
3. **Use a simple state management pattern.** The existing `state` object + `setState(patch)` + `render()` pattern in `settings.js` is adequate — replicate it in `dashboard.js` with section-specific dirty flags.
4. **Keep individual render functions under 40 lines.** When a render function grows, extract the item-level rendering into a helper.

**Detection:**
- Any single `.js` file exceeding 800 lines needs refactoring.
- Any single render function exceeding 50 lines is a code smell.

**Phase:** Dashboard implementation phase.

---

### Pitfall 10: Error Log Display Accumulating Without Bound

**What goes wrong:** If the dashboard shows an error log populated by `VfsEvent` emissions and tracing output, entries accumulate over the app's lifetime. After hours of operation with intermittent connectivity, the error log could have thousands of entries, consuming memory in both the Rust backend (if stored) and the WebView DOM.

**Prevention:**
1. **Cap the error log at 200 entries** in the Rust backend (ring buffer with `VecDeque`).
2. **Show only the last 50 entries in the DOM** with a "load more" mechanism.
3. **Group repeated errors.** "Upload failed: network error" appearing 50 times should show as "Upload failed: network error (×50)" with a timestamp range.
4. **Add severity levels** (error, warning, info) with filtering in the UI.

**Detection:**
- Monitor DOM node count on the dashboard page — should stay under 2000 nodes total.
- Test with simulated network flapping (online/offline every 5 seconds for 30 minutes).

**Phase:** Dashboard implementation phase.

---

### Pitfall 11: Cross-Platform Parity Gaps in Observability

**What goes wrong:** The FUSE backend has `inval_inode` (kernel cache invalidation via the session notifier) which the delta sync observer uses. WinFsp has no equivalent — the `WinFspDeltaObserver` only marks handles stale (line 104-105 of `winfsp_fs.rs`), with no kernel cache invalidation. This means observability metrics like "cache invalidations performed" would show different behavior per platform, potentially confusing the dashboard.

**Prevention:**
1. **Design metrics as platform-abstract.** Report "items refreshed" not "inval_inode calls." The metric should represent the logical operation (stale data detected and handled) not the platform mechanism.
2. **Explicitly document which metrics are platform-specific** in the dashboard UI (e.g., "kernel cache flushes" only shown on Linux/macOS).
3. **Test the dashboard on both platforms** to verify all sections display correct data.

**Detection:**
- Run the same file operations on Linux and Windows — compare dashboard output.
- Check for `#[cfg]`-gated metric collection code that might be missing on one platform.

**Phase:** Observability implementation phase.

---

### Pitfall 12: `showStatus()` Notification Overlap with Dashboard Status Display

**What goes wrong:** The existing `showStatus()` in `ui.js` uses a single `#status-bar` element with auto-dismiss (3 seconds for success/info). If the dashboard also shows status information (sync state, online/offline), there's a visual conflict: the status bar shows transient messages while the dashboard shows persistent state. Users may miss important transient errors because the dashboard's persistent state occupies their attention.

**Prevention:**
1. **Separate transient notifications from persistent status.** The status bar remains for action feedback ("Settings saved"). Dashboard panels show persistent state (sync status, online/offline). These should be visually distinct areas.
2. **Add notification importance levels.** Errors should be sticky (require dismiss) — this is already implemented for `type === 'error'` in `showStatus()`. Extend to dashboard context.

**Detection:**
- UX review: trigger a settings save while the dashboard is showing an offline status. Both should be visible simultaneously.

**Phase:** Dashboard UI implementation phase.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| WinFsp offline pin fix | **Pitfall 1** (Explorer hang), **Pitfall 6** (memory cache eviction breaks pins), **Pitfall 8** (slow `open_file` on degraded network) | Fix memory cache eviction protection first, then add short timeouts for VFS-path Graph calls, then verify Explorer navigation with network unplugged |
| main.rs refactor | **Pitfall 3** (lock ordering not documented) | Document lock ordering as part of the refactor. Extract dashboard state into a separate lock-free structure during the refactor, not after |
| Observability architecture | **Pitfall 2** (event flooding), **Pitfall 3** (lock contention), **Pitfall 4** (IPC overhead), **Pitfall 7** (delta sync competing for locks) | Use `watch` channels + atomic counters for metrics. Use `emit()` for push updates. Design the `DashboardState` struct before writing any dashboard code |
| Dashboard UI implementation | **Pitfall 5** (vanilla JS DOM performance), **Pitfall 9** (JS file size), **Pitfall 10** (error log unbounded), **Pitfall 12** (status overlap) | Use DocumentFragment, targeted updates, ring-buffered error log. Create `dashboard.js` as a separate file with section-specific rendering |
| Cross-platform testing | **Pitfall 11** (platform parity gaps) | Design metrics as platform-abstract. Test dashboard on both Linux and Windows before deployment |

## Sources

- Codebase analysis: `crates/carminedesktop-vfs/src/core_ops.rs` (1973 lines), `winfsp_fs.rs` (1168 lines), `fuse_fs.rs`, `sync_processor.rs` (685 lines)
- State management: `crates/carminedesktop-app/src/main.rs` (2167 lines) — `AppState` struct, delta sync loop, VFS event handler
- Frontend patterns: `crates/carminedesktop-app/dist/settings.js` (520 lines), `ui.js` (46 lines)
- Architecture: `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/CONCERNS.md`
- WinFsp behavior: `winfsp_fs.rs` callback implementations, offline flag handling
- Domain knowledge: FUSE `WRITEBACK_CACHE` kernel interaction, WinFsp FileSystemContext threading model, Tauri v2 IPC architecture

---

*Pitfalls audit: 2026-03-18*
