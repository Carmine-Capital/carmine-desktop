# Feature Landscape

**Domain:** Cloud storage desktop sync — observability, status dashboards, and offline file management
**Researched:** 2026-03-18
**Overall confidence:** HIGH (based on established competitor patterns + deep codebase analysis)

## Context: What Exists Today in CarmineDesktop

Before mapping features, here's what the codebase already provides as backend data sources:

| Data Source | Location | What It Provides |
|-------------|----------|------------------|
| `SyncMetrics` | `carminedesktop-vfs/sync_processor.rs` | `queue_depth`, `in_flight`, `failed_count`, `total_uploaded`, `total_failed`, `total_deduplicated` — live via `watch::Receiver` |
| `VfsEvent` | `carminedesktop-vfs/core_ops.rs` | `ConflictDetected`, `WritebackFailed`, `UploadFailed`, `FileLocked` — emitted via MPSC channel |
| `DeltaSyncResult` | `carminedesktop-cache/sync.rs` | `changed_items`, `deleted_items` per sync cycle |
| `DiskCache::total_size()` | `carminedesktop-cache/disk.rs` | Current cache disk usage in bytes |
| `DiskCache::max_size_bytes()` | `carminedesktop-cache/disk.rs` | Configured maximum cache size |
| `WriteBackBuffer::list_pending()` | `carminedesktop-cache/writeback.rs` | List of `(drive_id, item_id)` pairs awaiting upload |
| `PinStore::list_all()` | `carminedesktop-cache/pin_store.rs` | All offline pins with TTL and expiry |
| `AtomicBool` offline flag | Per mount in `mount_caches` | Whether a mount is online or offline |
| `auth_degraded` flag | `AppState` | Whether auth token refresh is failing |
| Tray menu | `tray.rs` | Shows mounted/unmounted/error status per drive |

**Current UI surface:** Settings page (General, Mounts, Offline, About tabs) + wizard. No dashboard. No activity view. No error log. No sync status beyond tray tooltip saying "N drive(s) mounted."

---

## Table Stakes

Features users expect from any cloud storage desktop app with a settings UI. Missing = product feels incomplete or untrustworthy for organizational deployment.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Per-drive sync status indicator** | OneDrive/Dropbox/Nextcloud all show "Up to date" / "Syncing" / "Error" per account/folder. Users need to know if their files are safe. | Low | Backend data exists: `SyncMetrics` has `queue_depth` + `in_flight`. Delta sync timing is known. Just needs a Tauri command to expose + UI to render. |
| **Online/offline status indicator** | OneDrive grays out icon when offline. Dropbox shows "Offline." Users panic when saves silently queue without feedback. | Low | Backend exists: `AtomicBool` offline flag per mount. Just expose to UI. Critical for org deployment — IT support needs this. |
| **Upload queue / pending changes count** | OneDrive shows "Processing changes" with count. Dropbox shows "Syncing N files." Users need to know writes are queued, not lost. | Low | `SyncMetrics::queue_depth` + `in_flight` already tracked. `WriteBackBuffer::list_pending()` gives file-level detail. |
| **Error display in UI** | OneDrive activity center shows per-file sync errors. Dropbox pops up conflict/error banners. Without this, users dig through logs or assume data loss. | Medium | `VfsEvent` channel already emits conflicts, upload failures, locked files. Need: ring buffer of recent events, Tauri command to list them, UI panel. |
| **Cache disk usage display** | OneDrive shows storage quota. Dropbox shows "Using X of Y." Users and IT admins need to know cache isn't eating their disk. | Low | `DiskCache::total_size()` and `max_size_bytes()` exist. Just expose and render a bar/gauge. |
| **Last synced timestamp** | Every sync client shows "Last synced: 2 minutes ago" or "Syncing..." Users use this as heartbeat — if it's stale, something is wrong. | Low | Not currently tracked. Need to store `Instant` after each successful `run_delta_sync`. Trivial to add. |
| **Auth status indicator** | OneDrive shows "Sign-in required" banner prominently. `auth_degraded` flag exists but only shows in tray. | Low | `auth_degraded` already tracked. Surface in dashboard header. |
| **Conflict notification with actionable info** | OneDrive shows conflict file names. Dropbox shows "Conflicted copy" banner with link. Users need to know which files need manual resolution. | Medium | `VfsEvent::ConflictDetected` fires with `file_name` and `conflict_name`. Need to surface these prominently, not just in a log. |

### Priority Order for Table Stakes

1. **Online/offline + auth status** — Zero effort, highest user anxiety reducer
2. **Per-drive sync status + last synced** — Heartbeat signal, "is it working?"
3. **Upload queue count** — "Are my saves safe?"
4. **Error display** — "What went wrong?" (but only after the above, since errors without context are worse)
5. **Cache disk usage** — IT admin concern, less urgent for individual users
6. **Conflict notification** — Already handled by file naming; UI just makes it discoverable

---

## Differentiators

Features that set CarmineDesktop apart from the native OneDrive client. Not expected, but valued — especially for IT admins doing organizational deployment.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Recent activity feed** | OneDrive buries activity in a tiny popup. A scrollable, filterable activity log (synced, uploaded, deleted, conflicted) in the dashboard gives IT admins and power users full transparency. | Medium | Needs a ring buffer of events (bounded, e.g. last 500). `DeltaSyncResult` gives changed/deleted items per cycle. `VfsEvent` gives upload events. Combine into unified activity model. |
| **Per-file upload progress** | OneDrive's sync client shows file-level progress for large uploads. The `SyncProcessor` already has per-inode tracking but doesn't expose byte-level progress. | High | Would require plumbing progress callbacks from `GraphClient::upload_large_file()` through `SyncProcessor` to UI. Chunked uploads (10MB chunks) could report per-chunk. Defer unless large file uploads are frequent. |
| **Offline pin health status** | Beyond listing pins: show whether pinned content is actually downloaded, partially downloaded, or stale. No competitor does this well — OneDrive just says "Always available" without health. | Medium | `PinStore` tracks pins. Need to cross-reference with `DiskCache` to verify actual content presence. `OfflineManager::redownload_changed_items` already handles staleness. |
| **Sync metrics over time** | Mini chart showing sync latency, upload throughput, error rate over the last hour/day. Useful for IT admins diagnosing network/performance issues. | High | Would need time-series storage (in-memory ring buffer or SQLite table). Overkill for stabilization milestone. |
| **Manual sync trigger per drive** | "Sync now" button per drive instead of waiting for the interval. Power user feature. | Low | `refresh_mount` command already exists and works. Just needs a UI button wired to it. Nearly free. |
| **Export diagnostic report** | One-click "export logs + state" for IT support. Bundles logs, config, cache stats, sync metrics into a zip. | Medium | Useful for org deployment. Would need to gather log files from rolling file, current config, cache stats. Not complex but many pieces. |
| **Writeback queue detail** | Show exactly which files are pending upload, with file names resolved from inodes. Helps users verify their saves are queued. | Medium | `WriteBackBuffer::list_pending()` gives `(drive_id, item_id)` pairs. Need to resolve to human-readable names via `SqliteStore::get_item_by_id()`. |

### Recommended Differentiators for This Milestone

1. **Manual sync trigger** — Nearly free, `refresh_mount` already exists
2. **Recent activity feed** — Medium effort, high value for transparency goal
3. **Offline pin health** — Aligns with fixing offline pin bugs; shows confidence in the fix
4. **Writeback queue detail** — Medium effort, directly addresses "are my saves safe?" anxiety

Defer: Per-file progress, sync metrics over time, diagnostic export (all can come in later milestones).

---

## Anti-Features

Features to explicitly NOT build for this stabilization milestone. Each has a clear reason.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Real-time file-change streaming to UI** | WebSocket/SSE push of every file operation would be noisy, complex, and mostly useless. OneDrive doesn't do this either — it shows aggregate status. | Poll `SyncMetrics` on a timer (e.g. every 2-5 seconds) via Tauri command. Batch event delivery. |
| **Per-file sync status overlay icons** | OneDrive/Dropbox achieve this via shell extensions (Windows) or Finder extensions (macOS). CarmineDesktop uses FUSE/WinFsp which doesn't integrate with shell icon overlays easily. Would require platform-specific shell extension DLLs. | Show file-level status in the dashboard UI instead. The VFS mount is inherently "cloud-first" (files are always the server version), unlike OneDrive's two-way sync model. |
| **Bandwidth throttling controls** | OneDrive and Dropbox offer upload/download speed limits. Implementing this requires wrapping all HTTP streams with rate limiters. Premature for stabilization. | Document as future feature. The current retry/backoff handles network congestion gracefully. |
| **Selective sync / folder exclusion** | OneDrive lets users choose which folders to sync. CarmineDesktop is a VFS (virtual filesystem) — everything is "on-demand" by nature. Selective sync doesn't apply to the VFS model. | Not needed. VFS architecture means files aren't downloaded until accessed. This is already better than selective sync. |
| **Version history / file restore UI** | OneDrive shows version history in the web UI. Adding this to the desktop app would require significant Graph API integration and UI work. | Link to "Open in SharePoint" (already exists via `open_online` command) where version history is available natively. |
| **Notification center / toast management UI** | Building a custom notification center when the OS already has one (Windows Action Center, macOS Notification Center). | Continue using `crate::notify` for OS-native notifications. Show persistent errors in the dashboard error panel only. |
| **Multi-account switching in dashboard** | CarmineDesktop v1 targets organizational M365 accounts only. Multi-account adds complexity for zero current value. | Single account display. Out of scope per PROJECT.md. |
| **Drag-and-drop file operations in dashboard** | The dashboard shows sync status, not a file browser. The actual filesystem (File Explorer / Finder) handles file operations. | Dashboard is for observability, not file management. |
| **Dark mode / theme customization** | Nice-to-have, but out of scope for stabilization. | Respect system theme if CSS supports `prefers-color-scheme`. Don't build a theme picker. |

---

## Feature Dependencies

```
Online/Offline Status  ───┐
Auth Status Indicator  ───┤
                          ├──→ Dashboard Shell (new page/tab)
Per-Drive Sync Status  ───┤       │
Last Synced Timestamp  ───┘       │
                                  ├──→ Full Dashboard
Upload Queue Count  ──────────────┤
Cache Disk Usage  ────────────────┤
                                  │
Error Display  ───────────────────┤
  └─ requires: VfsEvent ring buffer (new backend component)
                                  │
Conflict Notification  ───────────┘
  └─ requires: Error Display (conflicts are a type of error)

Recent Activity Feed ──→ requires: unified event model combining DeltaSyncResult + VfsEvent
  └─ depends on: Error Display infrastructure (ring buffer pattern)

Offline Pin Health ──→ requires: cross-reference PinStore with DiskCache content verification
  └─ depends on: Offline pin bug fix (WinFsp crash must be fixed first)

Manual Sync Trigger ──→ no dependencies (refresh_mount command exists)

Writeback Queue Detail ──→ requires: item_id → name resolution (SqliteStore lookup)
  └─ depends on: Upload Queue Count (UI component can extend)
```

### Critical Path

1. **Dashboard shell** (new HTML page or tab in settings) — everything depends on having a UI surface
2. **Backend event infrastructure** — ring buffer for VfsEvent + DeltaSyncResult aggregation
3. **Tauri commands** for exposing metrics — `get_sync_status`, `get_activity`, `get_cache_stats`
4. **Individual UI components** can then be built incrementally

---

## MVP Recommendation

Prioritize for this stabilization & observability milestone:

### Phase 1: Dashboard Shell + Status At-A-Glance
1. **Dashboard page** — New tab or replace settings landing with status overview
2. **Per-drive sync status** — "Up to date" / "Syncing N files" / "Offline" / "Error"
3. **Online/offline indicator** — Per mount, prominent
4. **Auth status** — Degraded auth banner
5. **Last synced timestamp** — Per drive
6. **Manual sync trigger** — "Sync Now" button (wires to existing `refresh_mount`)

### Phase 2: Activity & Errors
7. **Error panel** — Recent errors with actionable detail (file name, error type, timestamp)
8. **Conflict notifications** — Surfaced in error panel with "conflict copy created" detail
9. **Upload queue count** — "3 files uploading, 2 queued"
10. **Recent activity feed** — Last N synced/uploaded/deleted items

### Phase 3: Cache & Offline
11. **Cache disk usage** — Bar showing "2.1 GB / 5 GB used"
12. **Offline pin health** — Status per pin: "Downloaded" / "Partial" / "Stale"
13. **Writeback queue detail** — Which files are pending upload, by name

### Defer to Later Milestone
- Per-file upload progress bars
- Sync metrics over time (charts)
- Diagnostic report export
- Bandwidth throttling

**Rationale:** Phase 1 addresses "is it working?" (the #1 user anxiety). Phase 2 addresses "what went wrong?" Phase 3 addresses "how much space is it using?" — the natural order of user concern when deploying to an organization.

---

## Competitor Feature Matrix

Based on Microsoft OneDrive support documentation (verified via official support page, HIGH confidence) and established knowledge of Dropbox/Nextcloud/Google Drive desktop clients (MEDIUM confidence — from training data, consistent with documented patterns).

| Feature | OneDrive | Dropbox | Google Drive | Nextcloud | CarmineDesktop (target) |
|---------|----------|---------|--------------|-----------|------------------------|
| Tray icon sync status | Yes (6+ icon states) | Yes (syncing/paused/error) | Yes (syncing/done/error) | Yes (syncing/done/error) | Yes (partial — mounted/unmounted only) → **Enhance** |
| Per-file overlay icons | Yes (cloud/check/sync/error via shell extension) | Yes (green check/blue sync/red X) | Yes (cloud/offline pin) | Yes (sync/ok/error) | No → **Anti-feature** (VFS model) |
| Activity center/popup | Yes (click tray → file activity) | Yes (click tray → recent changes) | No (web only) | Yes (in settings dialog) | No → **Build** |
| Upload progress | Per-file in activity popup | Aggregate in popup | Minimal | Per-file in dialog | Aggregate count → **Phase 2** |
| Error detail in UI | Per-file error with resolution hint | Banner + per-file in popup | Minimal | Per-file in sync dialog | No → **Build** |
| Online/offline indicator | Grayed icon + tooltip | "Offline" text in popup | Grayed icon | Status text | Offline flag exists → **Expose** |
| Pause/resume sync | Yes (tray menu) | Yes (tray menu) | Yes (tray menu) | Yes (tray menu) | No → **Consider for later** |
| Cache/storage usage | Shows OneDrive quota | Shows Dropbox quota | Shows Drive quota | Shows server quota | `total_size` exists → **Build UI** |
| Selective sync | Yes (folder picker) | Yes (folder picker) | Yes (stream vs mirror) | Yes (folder picker) | N/A (VFS = all on-demand) |
| Offline/always available | Yes ("Always keep on device") | Yes (Smart Sync "Local") | Yes (offline pin) | Yes (VFS "Always available") | Yes (offline pin, buggy) → **Fix + enhance** |
| Conflict resolution UI | Renames + notification | Pop-up with both versions | Renames only | Dialog with options | Renames + `VfsEvent` → **Surface in UI** |
| Last synced timestamp | Yes (in activity popup) | Yes (in popup) | Not prominent | Yes (in sync dialog) | Not tracked → **Add** |
| Manual sync trigger | Not explicit (auto-continuous) | Not explicit (auto-continuous) | Not explicit | Yes (in dialog) | `refresh_mount` exists → **Add button** |
| Bandwidth controls | Yes | Yes | Yes | Yes | No → **Anti-feature for now** |
| Diagnostic/log export | Yes (via support tool) | Yes (via support) | No | Yes (export logs) | No → **Defer** |

---

## Sources

- Microsoft OneDrive sync icon documentation: https://support.microsoft.com/en-us/office/what-do-the-onedrive-icons-mean-11143026-8000-44f8-aaa9-67c985aa49b3 (HIGH confidence — official Microsoft support, verified 2026-03-18)
- Nextcloud desktop client documentation: https://docs.nextcloud.com/desktop/latest/ (MEDIUM confidence — official docs, minimal detail on specific UI features)
- Dropbox desktop client features: Training data knowledge of Dropbox sync client UX patterns (MEDIUM confidence — well-established patterns, not independently verified today)
- Google Drive for Desktop features: Training data (MEDIUM confidence — well-established)
- CarmineDesktop codebase: Direct code analysis (HIGH confidence — primary source)
