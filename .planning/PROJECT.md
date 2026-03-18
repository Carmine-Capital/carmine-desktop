# CarmineDesktop — Stabilization & Observability

## What This Is

CarmineDesktop mounts Microsoft OneDrive and SharePoint document libraries as local filesystems on Linux, macOS, and Windows. This milestone focuses on making the app robust enough for company-wide deployment: fixing the offline pin crash on Windows, adding a dashboard UI with sync observability, and polishing the overall user experience. Currently dogfooded by the developer on Linux; target deployment is Windows across the organization.

## Core Value

When something goes wrong, you know about it and can diagnose it — the app is transparent, not a black box.

## Requirements

### Validated

<!-- Shipped and confirmed valuable. Inferred from existing codebase. -->

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

### Active

<!-- Current scope. Building toward these. -->

- [ ] Dashboard UI showing sync state, file activity, errors, cache, and offline status at a glance
- [ ] Sync status display per drive (last synced, next sync, items pending upload/download)
- [ ] Upload/download activity display with progress indication
- [ ] Error log visible in UI with actionable detail (not just log files)
- [ ] Cache usage display (disk space consumed, pinned items size)
- [ ] Online/offline status indicator per drive
- [ ] Fix WinFsp offline pin crash (File Explorer hangs when navigating offline pinned mount)
- [ ] Sync observability: make sync behavior transparent without digging through log files
- [ ] UI visual polish: modernize look, improve usability, provide consistent user feedback

### Out of Scope

<!-- Explicit boundaries. Includes reasoning to prevent re-adding. -->

- New cloud providers (Google Drive, Dropbox) — OneDrive/SharePoint only for v1
- Personal Microsoft accounts — organizational M365 only per v1 constraint
- Mobile app — desktop-only product
- New sync features beyond fixing/observing existing behavior — stabilize first
- Headless mode improvements — desktop deployment is the priority
- New file operation features (e.g., shared links, version history UI) — stabilize core first

## Context

- **Brownfield:** Rust 2024 workspace with 6 crates, Tauri v2 desktop app, ~8000 lines of core VFS/cache logic. Fully functional but with rough edges.
- **Current UI:** Vanilla JS frontend (no framework, no build step) with two pages: `wizard.html` (setup) and `settings.html` (configuration). No dashboard or observability surface.
- **Known issues from codebase audit:**
  - WinFsp offline pin interaction causes File Explorer crash — likely VFS responses during offline state trigger Explorer hang
  - No UI indicator for online/offline state (VFS silently enters offline mode)
  - No cache size display or management UI
  - Memory cache eviction is O(n) scan of all entries
  - `OpenFileTable::find_by_ino` is O(n) scan — performance concern with many open files
  - Monolithic `main.rs` (2167 lines) — may need refactoring to add observability hooks
- **Deployment target:** Windows across the organization. Developer dogfoods on Linux. Cross-platform parity maintained.
- **CI:** GitHub Actions enforces zero warnings (`RUSTFLAGS=-Dwarnings`), clippy all targets, fmt check.
- **CSP constraint:** `script-src 'self'` — no inline event handlers in HTML.

## Constraints

- **Tech stack**: Rust + Vanilla JS + Tauri v2 — no framework migration
- **Cross-platform**: Changes must work on Linux (FUSE), macOS (macFUSE), and Windows (WinFsp)
- **Zero warnings**: CI enforces `RUSTFLAGS=-Dwarnings` — no suppressions without justification
- **CSP compliance**: No inline event handlers — `addEventListener` in `.js` files only
- **No build step**: Frontend remains vanilla JS served from `dist/` — no bundler, no framework
- **Backward compatibility**: Existing config.toml format and mount configurations must continue to work

## Key Decisions

<!-- Decisions that constrain future work. Add throughout project lifecycle. -->

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Keep vanilla JS frontend | No build step, fast iteration, matches existing codebase | — Pending |
| Dashboard replaces/extends settings page | Single UI surface rather than adding new windows | — Pending |
| Windows is primary deployment target | Company uses Windows; developer uses Linux for dev | — Pending |
| Fix offline pins before adding new features | Crash is a showstopper for rollout | — Pending |

---
*Last updated: 2026-03-18 after initialization*
