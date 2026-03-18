# Project Research Summary

**Project:** CarmineDesktop — Stabilization & Observability Milestone
**Domain:** Desktop VFS sync app (OneDrive/SharePoint → local filesystem) — observability dashboard + offline bug fix
**Researched:** 2026-03-18
**Confidence:** HIGH

## Executive Summary

CarmineDesktop is a mature Rust/Tauri v2 desktop app that mounts OneDrive/SharePoint as local filesystems via FUSE (Linux/macOS) and WinFsp (Windows). The stabilization & observability milestone has two distinct workstreams: **fixing a deployment-blocking WinFsp offline pin crash** (File Explorer hangs when navigating offline-pinned mounts) and **building an observability dashboard** so users and IT admins can see sync status, errors, cache usage, and activity in real time. Research confirms both workstreams can be completed with **zero new crate dependencies** — every capability needed (Tauri IPC Channels, `tokio::broadcast`, custom `tracing::Layer`, `tracing-appender` log rotation) already exists in the workspace.

The recommended approach is a **dual-mode architecture**: push-based events (via Tauri `emit()` or `Channel`) for real-time activity and errors, paired with pull-based Tauri `invoke()` commands for snapshot queries (cache stats, drive status). The existing `VfsEvent` channel for rare error events stays unchanged; a new `ObsEvent` enum in `carminedesktop-core` provides the unified observability event type. The dashboard is implemented as a new panel within the existing `settings.html` page (following the established vanilla JS pattern — no framework, no build step), making the tray click open directly to a status overview instead of settings.

The dominant risk is the **WinFsp offline pin crash**, which is the deployment blocker and must be fixed before observability work. Root cause analysis points to `CoreOps::resolve_path` calling `rt.block_on(graph.list_children(...))` during offline transitions — the first failing network call blocks a WinFsp thread for the full HTTP timeout (~30s), hanging Explorer. The fix requires: (1) protecting pinned items from memory cache eviction, (2) short-circuiting Graph API calls earlier in the offline transition, and (3) using aggressive timeouts for VFS-callback-path network calls. Secondary risks include event flooding (solved by using `watch` channels + atomic counters instead of per-operation events), lock contention between dashboard polls and the delta sync loop (solved by snapshot-then-release pattern and lock-free reads), and vanilla JS DOM performance (solved by targeted updates and `DocumentFragment`).

## Key Findings

### Recommended Stack

No new dependencies. The entire milestone is built on existing workspace crates. This is a significant advantage — zero supply chain risk increase, no new compilation time, no new API surfaces.

**Core technologies (all already in workspace):**
- **Tauri `ipc::Channel`**: Real-time backend→frontend streaming — purpose-built for push updates, faster than `emit()`, ordered delivery
- **`tokio::sync::broadcast`**: Multi-consumer event fanout — replaces single-consumer `mpsc` so dashboard, tray, and notifications all subscribe independently
- **Custom `tracing::Layer` + `VecDeque`**: Error/warning ring buffer — intercepts `warn!`/`error!` events as they happen, avoids fragile log file parsing
- **`tracing_appender::rolling::Builder`**: Log rotation with `max_log_files(31)` — fixes unbounded log growth (already in `tracing-appender 0.2.4`)
- **Vanilla JS + Tauri global API**: Dashboard UI following existing `settings.js` patterns — project constraint prohibits frameworks/build steps

**What NOT to use:** OpenTelemetry/metrics/prometheus (overkill for desktop app), React/Vue/Svelte (violates no-build-step constraint), WebSocket plugin (unnecessary — Channels are built in), Web Components (more ceremony than benefit).

### Expected Features

**Must have (table stakes):**
1. Online/offline status indicator per mount — zero effort, highest anxiety reducer
2. Per-drive sync status ("Up to date" / "Syncing N files" / "Error") — heartbeat signal
3. Last synced timestamp per drive — "is it working?" indicator
4. Auth status indicator — degraded auth banner in dashboard header
5. Upload queue / pending changes count — "are my saves safe?"
6. Error display with actionable detail — file name, error type, timestamp
7. Cache disk usage display — IT admin concern, bar showing "X / Y GB used"
8. Conflict notification surfaced in UI — not just desktop notifications

**Should have (differentiators for this milestone):**
1. Manual sync trigger ("Sync Now" button) — nearly free, `refresh_mount` command already exists
2. Recent activity feed — scrollable log of synced/uploaded/deleted/conflicted items
3. Offline pin health status — "Downloaded" / "Partial" / "Stale" per pin
4. Writeback queue detail — which files are pending upload, by name

**Defer (later milestones):**
- Per-file upload progress bars, sync metrics over time (charts), diagnostic report export, bandwidth throttling, pause/resume sync, dark mode/theme customization

**Anti-features (explicitly NOT building):**
- Per-file sync status overlay icons (requires shell extensions, doesn't fit VFS model)
- Selective sync / folder exclusion (VFS is inherently on-demand — already better)
- Real-time per-operation file streaming to UI (noisy, use aggregate counters instead)
- Drag-and-drop in dashboard (it's for observability, not file management)

### Architecture Approach

The observability layer is a **funnel architecture**: lower crates (`carminedesktop-vfs`, `carminedesktop-cache`) emit events through existing channels or expose query methods. The app layer (`carminedesktop-app`) is the single funnel that converts `VfsEvent` → `ObsEvent`, emits sync/auth events at lifecycle points, pushes to the Tauri frontend via `emit()`, and accumulates errors in a ring buffer. No lower crate needs to know about Tauri or the dashboard — the dependency graph is preserved.

**Major components:**
1. **`ObsEvent` enum** (in `carminedesktop-core`) — unified observability event type: activity, sync, state transitions, errors
2. **`EventBridge`** (in `carminedesktop-app`) — extends existing `spawn_event_forwarder` to also emit Tauri events and feed the ErrorAccumulator
3. **`ErrorAccumulator`** (in `carminedesktop-app`) — `VecDeque` ring buffer (200 entries max) behind `Mutex`, queried by `get_recent_errors` command
4. **Dashboard commands** (`get_dashboard_status`, `get_cache_stats`, `get_recent_errors`) — Tauri commands that snapshot state from `AppState` using lock-free reads where possible
5. **Dashboard UI** — new panel within `settings.html`, vanilla JS, uses `invoke()` for snapshots + `listen()` for live events
6. **Stat methods on existing types** — trivial getters: `MemoryCache::entry_count()`, `WriteBackBuffer::pending_count()`, `MountHandle::sync_metrics()`

### Critical Pitfalls

1. **WinFsp offline pin — Explorer hangs on blocking `resolve_path`** — Pinned folder metadata evicted from memory cache → `find_child` falls through to Graph API → 30s timeout blocks WinFsp thread → Explorer hangs. **Fix:** Protect pinned inodes from eviction, add short timeout (3-5s) for VFS-path Graph calls, pre-populate SQLite with full pinned folder tree.

2. **Event flooding from over-instrumentation** — Instrumenting every `read`/`write`/`lookup` generates thousands of events/sec, exhausting memory via unbounded channels. **Fix:** Use `watch` channels + atomic counters for metrics (not per-event MPSC). Keep `VfsEvent` for rare events only. Rate-limit frontend to 1-2 Hz.

3. **Lock contention between dashboard and delta sync** — Dashboard polls and delta sync both lock `mount_caches` + `effective_config`. **Fix:** Snapshot-then-release pattern (never hold two `AppState` mutexes simultaneously), use `AtomicBool`/`AtomicU64` for lock-free reads, document and enforce lock ordering.

4. **Tauri IPC overhead for real-time updates** — Polling 3-4 data sources at 1Hz via `invoke()` adds serialization overhead and queues behind busy Rust handlers. **Fix:** Use `emit()` for push-based incremental updates, `invoke()` only for initial load and user-triggered actions.

5. **Vanilla JS DOM performance without virtual DOM** — Full re-render on every state change (`innerHTML = ''` + `createElement()` loops) causes jank with 100+ activity items. **Fix:** Targeted updates (update `textContent`, append/remove items), `DocumentFragment` for batch insertions, cap visible items at 50-100.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: WinFsp Offline Pin Fix
**Rationale:** Deployment-blocking crash bug. Must be fixed before any feature work. Investigation may reveal VFS response patterns that inform observability event design.
**Delivers:** Stable offline navigation in File Explorer for pinned folders; log rotation fix.
**Addresses:** Offline pin crash (table stakes for organizational deployment), unbounded log growth.
**Avoids:** Pitfall 1 (Explorer hang), Pitfall 6 (memory cache eviction breaks pins), Pitfall 8 (blocking `open_file` on degraded network).
**Scope:**
- Add memory cache eviction protection for pinned inodes (mirror `DiskCache::is_protected()`)
- Short-circuit Graph API calls with aggressive timeouts (3-5s) in VFS callback paths
- Pre-populate SQLite with full pinned folder tree during `pin_folder`
- Check offline flag at each `resolve_path` step, not just inside `find_child`
- Implement `tracing-appender` Builder with `max_log_files(31)` (~5-line fix)
- Add `tracing::debug!` instrumentation to WinFsp `FileSystemContext` methods for investigation

### Phase 2: Observability Infrastructure
**Rationale:** Foundation for dashboard — defines the event types, data flow, and backend plumbing. Must be designed carefully to avoid event flooding and lock contention (Pitfalls 2, 3, 7). Doing this before UI ensures the data layer is queryable and testable independently.
**Delivers:** `ObsEvent` enum, `EventBridge`, `ErrorAccumulator`, stat methods on cache types, dashboard Tauri commands.
**Uses:** `tokio::sync::broadcast`, custom `tracing::Layer`, existing `VfsEvent` channel, `AppState` atomic fields.
**Implements:** EventBridge (Pattern 1), Snapshot Commands (Pattern 2), ErrorAccumulator (Pattern 3) from ARCHITECTURE.md.
**Avoids:** Pitfall 2 (event flooding — use `watch` channels, not per-event MPSC), Pitfall 3 (lock contention — snapshot-then-release, lock-free reads), Pitfall 7 (delta sync lock competition).
**Scope:**
- Define `ObsEvent` enum in `carminedesktop-core`
- Build `ErrorAccumulator` (ring buffer, 200 entries max) in `carminedesktop-app`
- Extend `spawn_event_forwarder` into `EventBridge` (VfsEvent → ObsEvent conversion + Tauri emit + error accumulation)
- Add stat methods: `MemoryCache::entry_count()`, `WriteBackBuffer::pending_count()`, `MountHandle::sync_metrics()`, `PinStore::list_pins()`
- Implement Tauri commands: `get_dashboard_status`, `get_cache_stats`, `get_recent_errors`
- Track `last_synced_at` per drive in delta sync loop
- Emit `ObsEvent` from delta sync, mount lifecycle, and auth state transitions

### Phase 3: Dashboard UI
**Rationale:** All backend data is now queryable and streamable. Build the UI incrementally: status overview first (highest user anxiety reduction), then activity/errors, then cache/offline details.
**Delivers:** Dashboard panel in settings page with drive status cards, activity feed, error log, cache usage, offline pin health.
**Addresses:** All table stakes features + recommended differentiators (manual sync, activity feed, pin health, writeback detail).
**Avoids:** Pitfall 5 (DOM performance — targeted updates, DocumentFragment), Pitfall 9 (JS file size — separate `dashboard.js`), Pitfall 10 (error log unbounded — capped display + ring buffer), Pitfall 12 (status overlap — separate transient notifications from persistent status).
**Scope:**
- Dashboard panel as default view for authenticated users (replaces settings landing)
- Nav: Dashboard | Mounts | General | Advanced
- Drive status cards: name, mountpoint, online/offline, "Up to date" / "Syncing N" / "Error", last synced
- "Sync Now" button per drive (wires to `refresh_mount`)
- Auth status banner (degraded auth warning)
- Activity feed: scrolling list of recent events (last 50 in JS), append-top/remove-bottom pattern
- Error log panel: recent errors from `get_recent_errors`, with severity and file detail
- Cache usage: bar showing disk cache used/max, pending upload count, writeback queue detail
- Offline pin health: per-pin status (Downloaded / Partial / Stale)
- Utility helpers in `ui.js`: `formatBytes()`, `formatRelativeTime()`, `createStatCard()`, `createActivityEntry()`

### Phase 4: Polish & Cross-Platform Validation
**Rationale:** Functionality complete; visual refinement, platform parity testing, and edge case hardening.
**Delivers:** Production-ready dashboard, verified on both Linux and Windows.
**Avoids:** Pitfall 11 (cross-platform parity gaps — platform-abstract metrics, test on both platforms).
**Scope:**
- Visual polish: status indicator colors (`--color-online`, `--color-offline`, `--color-syncing`, `--color-error`), responsive layout
- Cross-platform testing: verify dashboard on Linux (FUSE) and Windows (WinFsp)
- Error grouping: collapse repeated errors ("Upload failed: network error ×50")
- Edge case testing: network flapping, bulk operations during dashboard display, long uptime (memory stability)
- Tray icon enhancement: update tooltip/icon based on sync state and online/offline (consumes `ObsEvent` via broadcast subscriber)

### Phase Ordering Rationale

- **Phase 1 before everything** — it's a crash/hang bug blocking deployment. Zero dependency on observability. Also, investigation reveals VFS behavior patterns that inform event design.
- **Phase 2 before Phase 3** — data layer must be queryable before building the view. Commands can be tested via browser console (`invoke()`) before any UI exists.
- **Phase 3 as a single phase** — all UI features depend on the same infrastructure (EventBridge + commands). Building incrementally within the phase (status → activity → cache → pins) follows the natural order of user concern: "is it working?" → "what went wrong?" → "how much space?"
- **Phase 4 last** — polish and cross-platform validation after functionality is solid. Catching parity gaps earlier wastes effort if the architecture changes.
- **Grouping rationale:** Architecture research shows clear dependency layers: VFS fix → event infrastructure → commands → UI → polish. Feature research confirms the user-concern ordering: online/offline → sync status → errors → cache. These align.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1:** WinFsp offline behavior is MEDIUM confidence — the debugging approach is sound but the actual root cause is unknown. Investigation may reveal unexpected causes. Plan for exploratory time.
- **Phase 3:** Dashboard UX patterns for status displays — may benefit from a brief competitor analysis sprint, though FEATURES.md competitor matrix covers the essentials.

Phases with standard patterns (skip research-phase):
- **Phase 2:** Observability infrastructure uses well-documented patterns (`tokio::broadcast`, custom `tracing::Layer`, Tauri commands). All APIs verified against official docs. HIGH confidence.
- **Phase 4:** Polish and testing are execution-focused, no research needed.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Zero new dependencies — all capabilities verified against official docs (Tauri v2, tracing-appender 0.2.4, tokio broadcast). No supply chain risk. |
| Features | HIGH | Table stakes derived from competitor analysis (OneDrive, Dropbox, Nextcloud) + deep codebase analysis confirming backend data availability. Existing `SyncMetrics`, `VfsEvent`, cache stats cover most needs. |
| Architecture | HIGH | Dual-mode push/pull pattern matches existing codebase patterns (`VfsEvent` MPSC, `SyncMetrics` watch, `invoke()` commands). EventBridge extends existing `spawn_event_forwarder`. No architectural leap required. |
| Pitfalls | HIGH | Derived from codebase analysis (lock patterns, cache eviction, IPC paths) and domain expertise (FUSE/WinFsp threading models). Critical pitfalls have concrete prevention strategies. |

**Overall confidence:** HIGH

### Gaps to Address

- **WinFsp offline crash root cause:** The analysis identifies 3 likely causes but the actual root cause is unconfirmed. Phase 1 must begin with investigation before committing to a fix strategy. Budget exploratory time.
- **Tauri Channel vs emit() performance:** STACK.md recommends Channels for streaming, ARCHITECTURE.md uses `emit()`. Both work — the choice should be made during Phase 2 implementation based on actual IPC benchmarking. Channels are theoretically faster but `emit()` is simpler and already used in the codebase.
- **Lock ordering in `AppState`:** 56 occurrences of `.lock().unwrap()` across the codebase, with at least one known ordering violation in the delta sync loop (lines 1519-1520 of `main.rs`). Lock ordering documentation should happen during Phase 2, not deferred.
- **Dashboard as panel vs separate page:** ARCHITECTURE.md recommends a panel within `settings.html`. This is the right call for simplicity, but if dashboard JS exceeds ~500 lines, extracting to a separate `dashboard.js` file loaded via `<script>` tag in `settings.html` is the escape hatch.

## Sources

### Primary (HIGH confidence)
- Tauri v2 — Calling Frontend from Rust (Channels): https://v2.tauri.app/develop/calling-frontend/#channels
- Tauri v2 — Calling Rust from Frontend: https://v2.tauri.app/develop/calling-rust/#channels
- `tracing-appender` 0.2.4 Builder API: https://docs.rs/tracing-appender/0.2.4/tracing_appender/rolling/struct.Builder.html
- `tracing-subscriber` 0.3.23 Layer composability: https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/
- `tokio::sync::broadcast`: https://docs.rs/tokio/latest/tokio/sync/broadcast/
- Microsoft OneDrive sync icons: https://support.microsoft.com/en-us/office/what-do-the-onedrive-icons-mean-11143026-8000-44f8-aaa9-67c985aa49b3
- CarmineDesktop codebase: direct code analysis of all 6 crates

### Secondary (MEDIUM confidence)
- WinFsp NTFS Compatibility wiki: https://github.com/winfsp/winfsp/wiki/NTFS-Compatibility
- WinFsp Debugging Setup wiki: https://github.com/winfsp/winfsp/wiki/WinFsp-Debugging-Setup
- Nextcloud desktop client docs: https://docs.nextcloud.com/desktop/latest/
- Dropbox/Google Drive desktop client UX patterns: training data knowledge, consistent with documented patterns

---
*Research completed: 2026-03-18*
*Ready for roadmap: yes*
