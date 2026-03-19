# CarmineDesktop — Stabilization & Observability

## What This Is

CarmineDesktop mounts Microsoft OneDrive and SharePoint document libraries as local filesystems on Linux, macOS, and Windows. After v1.0, the app is robust enough for company-wide deployment: offline pin crash on Windows is fixed, a dashboard UI provides full sync observability, and the UI is modernized with a consistent dark theme.

## Core Value

When something goes wrong, you know about it and can diagnose it — the app is transparent, not a black box.

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

- ✓ OAuth2 PKCE authentication with Microsoft 365 organizational accounts — existing
- ✓ Mount OneDrive drives as local filesystem (FUSE on Linux/macOS, WinFsp on Windows) — existing
- ✓ Mount SharePoint document libraries as local filesystem — existing
- ✓ Multi-tier caching: memory (DashMap) → SQLite → disk with LRU eviction — existing
- ✓ Read/write files through virtual filesystem with conflict detection — existing
- ✓ Delta sync at configurable interval (default 60s) — existing
- ✓ Write-back buffer with crash-safe persistence and async upload queue — existing
- ✓ Setup wizard for first-run onboarding and authentication — existing
- ✓ Settings UI for mount point and drive configuration — existing
- ✓ System tray with mount controls and status — existing
- ✓ Offline folder pinning with recursive download — existing (buggy on Windows)
- ✓ Secure token storage via OS keychain with AES-256-GCM encrypted fallback — existing
- ✓ Autostart on login (systemd, LaunchAgent, Registry) — existing
- ✓ Auto-updater via tauri-plugin-updater — existing
- ✓ Shell integration: file associations, context menus, Explorer nav pane — existing
- ✓ WinFsp offline pin crash fixed (5s VFS timeout + eviction protection + SQLite metadata) — v1.0
- ✓ Dashboard UI with sync state, activity, errors, cache, offline status at a glance — v1.0
- ✓ Per-drive sync status display (last synced, syncing, error indicators) — v1.0
- ✓ Upload queue and writeback queue detail visible in UI — v1.0
- ✓ Error log in UI with actionable detail (file, type, timestamp) — v1.0
- ✓ Cache disk usage display (current vs. max) — v1.0
- ✓ Online/offline status indicator per drive — v1.0
- ✓ Conflict notifications surfaced in dashboard — v1.0
- ✓ Offline pin health badges (Downloaded/Partial/Scanning) — v1.0
- ✓ Auth degraded banner when token refresh failing — v1.0
- ✓ UI visual polish: soft dark palette, consolidated typography, normalized spacing — v1.0
- ✓ Real-time dashboard updates via ObsEvent bus — v1.0

### Active

<!-- Next milestone scope. -->

(None yet — define with `/gsd:new-milestone`)

### Out of Scope

<!-- Explicit boundaries. Includes reasoning to prevent re-adding. -->

- New cloud providers (Google Drive, Dropbox) — OneDrive/SharePoint only for v1
- Personal Microsoft accounts — organizational M365 only per v1 constraint
- Mobile app — desktop-only product
- Per-file sync status overlay icons — VFS model doesn't integrate with shell icon overlay providers
- Real-time file-change streaming to UI — poll-based SyncMetrics with batched events sufficient
- Selective sync / folder exclusion — VFS on-demand architecture makes this unnecessary
- Version history / file restore UI — available via "Open in SharePoint"
- Dark mode / theme customization — CSS custom properties allow future theming, but not prioritized
- Pause/resume sync — defer to future milestone
- Notification center UI — OS notifications + dashboard error panel sufficient

## Context

- **Shipped v1.0** with 26,187 LOC Rust + 2,920 LOC JS/HTML/CSS across 6 crates
- **Tech stack:** Rust 2024, Tauri v2, Vanilla JS (no framework, no build step), FUSE/WinFsp
- **Dashboard:** 6-section panel (drive cards, upload queue, activity feed, error log, cache & offline) with real-time obs-event updates
- **Deployment target:** Windows across the organization. Developer dogfoods on Linux.
- **CI:** GitHub Actions enforces zero warnings (`RUSTFLAGS=-Dwarnings`), clippy all targets
- **Known limitations:**
  - Activity feed only shows delta-sync-driven events (not local writes or offline downloads)
  - `OpenFileTable::find_by_ino` is O(n) scan — performance concern with many open files
  - Memory cache eviction is O(n) scan of all entries

## Constraints

- **Tech stack**: Rust + Vanilla JS + Tauri v2 — no framework migration
- **Cross-platform**: Changes must work on Linux (FUSE), macOS (macFUSE), and Windows (WinFsp)
- **Zero warnings**: CI enforces `RUSTFLAGS=-Dwarnings` — no suppressions without justification
- **CSP compliance**: No inline event handlers — `addEventListener` in `.js` files only
- **No build step**: Frontend remains vanilla JS served from `dist/` — no bundler, no framework
- **Backward compatibility**: Existing config.toml format and mount configurations must continue to work

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Keep vanilla JS frontend | No build step, fast iteration, matches existing codebase | ✓ Good — dashboard delivered in 2 plans without tooling overhead |
| Dashboard replaces/extends settings page | Single UI surface rather than adding new windows | ✓ Good — dashboard is default panel, settings/mounts/offline are tabs |
| Windows is primary deployment target | Company uses Windows; developer uses Linux for dev | ✓ Good — offline pin crash fixed, cross-platform parity maintained |
| Fix offline pins before adding new features | Crash is a showstopper for rollout | ✓ Good — VFS timeout + eviction protection + SQLite metadata solved it |
| Zero new dependencies for observability | All capabilities exist in workspace (Tauri IPC, tokio broadcast, tracing) | ✓ Good — no dep additions, ring buffers + broadcast channel sufficient |
| graph_with_timeout centralizes VFS-path timeouts | Consistent 5s timeout + offline-flag logic, avoids duplication | ✓ Good — single helper wraps all 6 VFS Graph call sites |
| ObsEvent bus with ring buffers | Decouples producers (VFS, delta sync) from consumers (dashboard, Tauri emit) | ✓ Good — verified end-to-end from browser console |
| 30s periodic dashboard refresh | Balances data freshness vs. IPC overhead | ✓ Good — real-time events for immediate changes + periodic for staleness |
| Lock ordering documented on AppState | Prevents deadlocks as observability adds more Mutex state | ✓ Good — no deadlocks encountered |
| PinStore::health() uses recursive CTE | Joins items and cache_entries without Graph API calls | ✓ Good — fast, accurate pin health from SQLite alone |

---
*Last updated: 2026-03-19 after v1.0 milestone*
