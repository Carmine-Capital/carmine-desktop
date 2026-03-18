# Technology Stack — Observability, Dashboard & Offline Fix

**Project:** CarmineDesktop — Stabilization & Observability Milestone
**Researched:** 2026-03-18
**Mode:** Stack dimension for existing brownfield Rust/Tauri v2 app

## Recommended Stack Additions

This is not a greenfield stack. CarmineDesktop already has a mature Rust 2024 workspace with `tracing 0.1`, `tracing-subscriber 0.3`, `tracing-appender 0.2`, Tauri v2, and vanilla JS. This document covers only **new** libraries and patterns needed for the observability/dashboard milestone.

### Backend Event Streaming (Rust → Frontend)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tauri::ipc::Channel` | (built into `tauri 2`) | Stream sync status, file activity, errors from Rust to dashboard in real-time | **Channels are Tauri v2's recommended mechanism for streaming data.** They are ordered, fast, and designed for exactly this use case (progress updates, status streams). The existing event system (`app.emit()`) works but is slower — events serialize to JSON and evaluate JS directly. Channels use optimized IPC. Already available — no new dependency. |
| `tokio::sync::broadcast` | (built into `tokio 1.50`) | Fan-out observability events from VFS/sync to multiple consumers (dashboard, tray, notifications) | The existing `mpsc::unbounded_channel` for `VfsEvent` is single-consumer — only the notification forwarder reads it. A `broadcast` channel lets the dashboard, tray updater, and notification system all subscribe independently. Already available — no new dependency. |

**Confidence:** HIGH — verified against official Tauri v2 docs (https://v2.tauri.app/develop/calling-frontend/#channels, retrieved 2026-03-18). Channels are documented as "designed to be fast and deliver ordered data" and explicitly recommended over the event system for streaming.

**Pattern detail — Tauri Channels:**

On the Rust side, a `Channel<T>` parameter in a `#[tauri::command]` function lets the backend push typed messages. On the JS side, you create a `Channel` object via `new Channel()`, set `onmessage`, and pass it to `invoke()`. This replaces the poll-based pattern of calling `invoke('get_sync_status')` on a timer.

```rust
// Rust side: dashboard status stream
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
enum DashboardEvent {
    SyncStatus { drive_id: String, last_synced: String, pending_uploads: usize },
    FileActivity { file_name: String, operation: String, status: String },
    Error { message: String, drive_id: Option<String> },
    CacheStats { disk_bytes: u64, pinned_bytes: u64, memory_entries: usize },
}

#[tauri::command]
async fn subscribe_dashboard(on_event: tauri::ipc::Channel<DashboardEvent>) {
    // Subscribe to broadcast channel, forward to frontend
}
```

```javascript
// JS side: dashboard listener
const onEvent = new window.__TAURI__.core.Channel();
onEvent.onmessage = (msg) => {
    switch (msg.event) {
        case 'syncStatus': updateSyncPanel(msg.data); break;
        case 'fileActivity': appendActivityLog(msg.data); break;
        case 'error': appendErrorLog(msg.data); break;
        case 'cacheStats': updateCacheDisplay(msg.data); break;
    }
};
await invoke('subscribe_dashboard', { onEvent });
```

### Observability Infrastructure (Rust Backend)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tracing-appender` Builder API | `0.2.4` (already `0.2` in workspace) | Add `max_log_files()` for log rotation | Existing code uses `tracing_appender::rolling::daily()` with no max files — logs grow unbounded (flagged in CONCERNS.md). The `Builder` API on `0.2.x` supports `.max_log_files(n)` to auto-delete old log files. **No version bump needed** — the feature is in the version already used. |
| Custom `tracing::Layer` | (built into `tracing-subscriber 0.3`) | Capture recent errors/warnings into a ring buffer for UI display | Instead of re-parsing log files, a custom Layer intercepts `warn!`/`error!` events as they happen and stores the last N in a bounded `VecDeque` behind a `Mutex`. The dashboard command reads from this buffer. Standard `tracing-subscriber` pattern — no new dependency. |

**Confidence:** HIGH — verified `tracing-appender 0.2.4` docs (https://docs.rs/tracing-appender/0.2.4/tracing_appender/rolling/struct.Builder.html, retrieved 2026-03-18). The `max_log_files` method exists on `Builder` in `0.2.4`.

**What NOT to use:**
- **`tracing-loki`/`tracing-opentelemetry`/`opentelemetry-*`**: Overkill. This is a desktop app deployed to ~50 Windows machines, not a cloud service. Local observability (ring buffer + dashboard) is the right scope. Adding OpenTelemetry would add 10+ transitive dependencies and a collector requirement.
- **`metrics`/`prometheus`**: Same reasoning — no metrics server to scrape. Internal counters exposed via Tauri commands are simpler and sufficient.
- **`log` crate**: The project already uses `tracing` everywhere. Don't mix logging ecosystems.

### Dashboard UI (Vanilla JS)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Vanilla JS + `window.__TAURI__` | N/A | Dashboard page rendered with DOM manipulation | Constraint from PROJECT.md: "No build step, frontend remains vanilla JS served from `dist/`". The existing `settings.js` demonstrates the pattern: `state` object + `render()` functions + `addEventListener` delegation. Dashboard follows the same architecture. |
| CSS Custom Properties | N/A | Dashboard theming and status indicator colors | Already used in `styles.css`. Extend with `--color-online`, `--color-offline`, `--color-syncing`, `--color-error` for status badges. |

**Confidence:** HIGH — this is not a technology choice but a constraint. The existing codebase proves the pattern works at the complexity level of a settings page with 5+ panels.

**Pattern detail — Vanilla JS Dashboard Architecture:**

Follow the existing `settings.js` pattern exactly:
1. **State object**: `const state = { syncStatus: {}, activity: [], errors: [], cacheStats: {}, activeTab: 'overview' }`
2. **Render functions**: `renderOverview()`, `renderActivity()`, `renderErrors()`, `renderCache()` — each reads from `state` and updates DOM
3. **Event delegation**: Single click handler on `.main-content` using `data-action` attributes
4. **Backend connection**: Tauri Channel for real-time streaming + `invoke()` for initial state load
5. **CSP compliance**: All event handlers via `addEventListener` — no inline handlers (CI constraint)

**What NOT to use:**
- **React/Vue/Svelte/Lit**: Explicitly out of scope. Adding a framework requires a build step, which violates the project constraint. The dashboard is a status display, not a complex interactive app.
- **Web Components**: Tempting for encapsulation, but adds complexity without proportional benefit for a single-page dashboard. `CustomElement` registration and shadow DOM are more ceremony than the existing pattern needs.
- **Canvas/D3.js**: No charts needed in v1. Sync status is text + badges + progress bars, all achievable with HTML/CSS. If charting is needed later, it can be added incrementally.

### WinFsp Offline Mode Debugging

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| WinFsp `winfsp-tests` | (bundled with WinFsp SDK) | Validate VFS behavior outside File Explorer | WinFsp ships test tools (`winfsp-tests-x64.exe`) that exercise the filesystem without going through Explorer's shell namespace extensions. This isolates whether the crash is in VFS callbacks or in Explorer's response to VFS replies. |
| Windows `DebugView` / `TraceView` | N/A | Capture WinFsp kernel driver debug output | WinFsp's kernel driver emits debug output via `DbgPrint`. DebugView from Sysinternals captures this. Essential for seeing what the kernel-mode driver does when Explorer hangs. |
| `tracing` instrumentation in `winfsp_fs.rs` | (existing) | Add `tracing::debug!` spans to all `FileSystemContext` methods | The WinFsp backend (`winfsp_fs.rs`, 1168 lines) has less tracing instrumentation than the FUSE backend. Adding spans to `get_security_by_name`, `open`, `read_directory`, and `cleanup` will reveal which callback Explorer is blocking on. |

**Confidence:** MEDIUM — the offline pin crash diagnosis approach is informed by WinFsp documentation and community patterns, but the actual root cause is unknown. The debugging tools are verified (WinFsp wiki, Sysinternals tools exist), but the fix path depends on what the investigation reveals.

**WinFsp Offline Pin Crash — Analysis Framework:**

The crash manifests as: "File Explorer hangs when navigating offline pinned mount." Based on codebase analysis, likely causes:

1. **`rt.block_on()` in VFS callback blocks indefinitely**: When offline, Graph API calls fail. If the timeout/offline detection doesn't kick in before the VFS thread pool is exhausted, Explorer hangs waiting for a response. The `offline_flag` (`AtomicBool`) should short-circuit Graph calls, but race conditions during the transition to offline mode could leave some calls in-flight.

2. **Explorer's shell namespace integration conflicts with offline state**: Explorer may issue `get_security_by_name` or `read_directory` calls that trigger behavior different from regular file access. The WinFsp NTFS compatibility docs confirm WinFsp supports directory change notifications and security queries — if the VFS returns unexpected errors for these during offline mode, Explorer could hang.

3. **`STATUS_IO_TIMEOUT` vs `STATUS_IO_DEVICE_ERROR` response**: The WinFsp backend returns `STATUS_IO_DEVICE_ERROR` for generic errors. During offline mode, returning a more specific NTSTATUS (like `STATUS_NETWORK_UNREACHABLE`) might give Explorer a better signal to show an offline indicator rather than hanging.

**Debugging approach (no new dependencies):**
- Add structured `tracing::debug!` spans to every `FileSystemContext` method in `winfsp_fs.rs`
- Reproduce the hang with `RUST_LOG=debug` and inspect which callback is blocking
- Use Sysinternals Process Monitor to see what Explorer requests vs what WinFsp responds
- Test with `winfsp-tests` to isolate Explorer-specific behavior

### Log Rotation Fix

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tracing_appender::rolling::Builder` | `0.2.4` | Replace `rolling::daily()` with Builder that sets `max_log_files(30)` | Direct fix for the "No log rotation or size cap" concern. The Builder API is in the already-used version. Change is ~5 lines in `main.rs`. |

**Confidence:** HIGH — verified from docs.rs, `max_log_files` is available on the Builder in `tracing-appender 0.2.4`.

**Current code (line ~444 in main.rs):**
```rust
let file_appender = tracing_appender::rolling::daily(log_dir, "carminedesktop.log");
```

**Fix:**
```rust
let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
    .rotation(tracing_appender::rolling::Rotation::DAILY)
    .filename_prefix("carminedesktop.log")
    .max_log_files(31) // keep ~1 month of logs
    .build(log_dir)
    .expect("failed to initialize log appender");
```

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Backend→Frontend streaming | Tauri `Channel` | Tauri `emit()` event system | Events serialize to JSON and evaluate JS directly — slower, unordered, no backpressure. Channels are purpose-built for streaming. |
| Backend→Frontend streaming | Tauri `Channel` | WebSocket plugin (`tauri-plugin-websocket`) | Adds unnecessary dependency. Channels are built into Tauri v2 core and do exactly the same thing with less overhead. |
| Multi-consumer event bus | `tokio::broadcast` | Custom `Arc<Mutex<Vec<Sender>>>` | `broadcast` is battle-tested, supports multiple subscribers, handles slow consumers via bounded buffer. Re-implementing this is waste. |
| Error buffer for UI | Custom `tracing::Layer` + `VecDeque` | Parse log files from frontend | Fragile — log file format changes break parsing. Log files are rotated (deleted). Ring buffer in memory is instant, structured, and decoupled from file format. |
| Error buffer for UI | Custom `tracing::Layer` | `tracing-subscriber` JSON format + read from file | Same fragility as above, plus JSON log format changes existing log output which may break other consumers. |
| Dashboard UI | Vanilla JS DOM | Lit / Web Components | Adds complexity without benefit. The dashboard is a status display, not a CRUD app. The existing `settings.js` pattern handles the complexity level needed. |
| Dashboard UI | Vanilla JS DOM | htmx | Requires a server. Tauri's IPC is not HTTP — htmx can't invoke Tauri commands directly. |
| WinFsp debugging | `tracing` + Sysinternals | WinDbg kernel debugging | Nuclear option. Start with user-mode tracing. WinDbg is a fallback if the callback-level tracing doesn't reveal the hang. |
| Log rotation | `tracing-appender` Builder | External logrotate / cron | Not cross-platform. Windows has no logrotate. The built-in `max_log_files` works everywhere. |

## No New Dependencies Required

The entire observability milestone can be built with **zero new crate dependencies**:

| Need | Solved By | Already In Workspace |
|------|-----------|---------------------|
| Real-time backend→frontend streaming | `tauri::ipc::Channel` | `tauri 2` |
| Multi-consumer event fanout | `tokio::sync::broadcast` | `tokio 1.50` |
| Error/warning ring buffer | Custom `tracing::Layer` + `std::collections::VecDeque` | `tracing-subscriber 0.3` |
| Log file rotation | `tracing_appender::rolling::Builder::max_log_files()` | `tracing-appender 0.2` |
| Dashboard UI | Vanilla JS + Tauri global API | `dist/` |
| Sync status data model | `serde::Serialize` structs | `serde 1.0` |
| WinFsp debugging | Enhanced `tracing::debug!` instrumentation | `tracing 0.1` |

This is a significant advantage — no supply chain risk increase, no new compilation time, no new API surfaces to learn.

## Key Patterns

### Pattern 1: Observability Event Bus

The current architecture has a single-consumer `mpsc::UnboundedSender<VfsEvent>` per mount that only feeds desktop notifications. The observability milestone needs multiple consumers.

**Recommended change:** Replace per-mount `mpsc::unbounded_channel` with a shared `tokio::sync::broadcast::Sender<ObservabilityEvent>` where `ObservabilityEvent` is a superset of `VfsEvent` that also includes sync status updates, cache stats, and delta sync progress.

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "data")]
pub enum ObservabilityEvent {
    // Existing VfsEvent variants
    ConflictDetected { file_name: String, conflict_name: String },
    UploadFailed { file_name: String, reason: String },
    FileLocked { file_name: String },
    WritebackFailed { file_name: String },
    // New observability variants
    SyncStarted { drive_id: String },
    SyncCompleted { drive_id: String, items_changed: usize },
    SyncFailed { drive_id: String, error: String },
    OfflineTransition { drive_id: String, is_offline: bool },
    UploadProgress { file_name: String, bytes_sent: u64, total_bytes: u64 },
    DownloadProgress { file_name: String, bytes_received: u64, total_bytes: u64 },
}
```

Subscribers:
- **Notification forwarder** (existing, refactored): Filters for error events, shows desktop notifications
- **Dashboard stream** (new): Forwards all events to the Tauri Channel for the dashboard UI
- **Tray updater** (new): Updates tray icon/tooltip based on sync status and online/offline state

### Pattern 2: Polling + Streaming Hybrid for Dashboard

Some data is event-driven (file activity, errors) and some is periodic state (cache size, memory usage). Use Tauri Channels for event streaming and `invoke()` for periodic state polling.

```javascript
// On dashboard init:
// 1. Load initial state via invoke()
const status = await invoke('get_dashboard_state');
renderDashboard(status);

// 2. Subscribe to real-time events via Channel
const onEvent = new Channel();
onEvent.onmessage = (msg) => applyEvent(msg);
await invoke('subscribe_dashboard', { onEvent });

// 3. Poll slow-changing state on interval (cache size, etc.)
setInterval(async () => {
    const stats = await invoke('get_cache_stats');
    updateCachePanel(stats);
}, 10000); // every 10 seconds
```

### Pattern 3: WinFsp NTSTATUS Offline Responses

When `offline_flag` is true, VFS callbacks should return appropriate NTSTATUS codes instead of attempting Graph API calls:

```rust
// In winfsp_fs.rs FileSystemContext methods:
if self.ops.is_offline() {
    // For read operations on cached files: serve from cache (already implemented in CoreOps)
    // For operations requiring network: return appropriate error
    return Err(STATUS_NETWORK_UNREACHABLE); // or STATUS_IO_DEVICE_ERROR
}
```

The key insight: Explorer handles `STATUS_NETWORK_UNREACHABLE` differently from a hanging callback. Returning an error immediately is always better than blocking on a network timeout.

## Installation

No new packages to install. All capabilities come from existing workspace dependencies.

```bash
# Verify current versions match expectations
cargo metadata --format-version 1 | jq '.packages[] | select(.name | startswith("tracing")) | {name, version}'
```

## Sources

- Tauri v2 — Calling Frontend from Rust (Channels section): https://v2.tauri.app/develop/calling-frontend/#channels — **HIGH confidence**, official docs, retrieved 2026-03-18
- Tauri v2 — Calling Rust from Frontend (Commands + Channels): https://v2.tauri.app/develop/calling-rust/#channels — **HIGH confidence**, official docs, retrieved 2026-03-18
- `tracing-appender` 0.2.4 Builder API: https://docs.rs/tracing-appender/0.2.4/tracing_appender/rolling/struct.Builder.html — **HIGH confidence**, crate docs, retrieved 2026-03-18
- `tracing-subscriber` 0.3.23 Layer composability: https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/ — **HIGH confidence**, crate docs, retrieved 2026-03-18
- WinFsp NTFS Compatibility: https://github.com/winfsp/winfsp/wiki/NTFS-Compatibility — **HIGH confidence**, official wiki, retrieved 2026-03-18
- WinFsp Debugging Setup: https://github.com/winfsp/winfsp/wiki/WinFsp-Debugging-Setup — **MEDIUM confidence**, official wiki exists but content not fully loaded
- tokio::sync::broadcast: https://docs.rs/tokio/latest/tokio/sync/broadcast/ — **HIGH confidence**, standard Tokio module, in-use workspace version
