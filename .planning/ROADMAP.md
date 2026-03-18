# Roadmap: CarmineDesktop — Stabilization & Observability

**Created:** 2026-03-18
**Granularity:** Coarse
**Phases:** 4
**Coverage:** 15/15 v1 requirements mapped

## Phases

- [x] **Phase 1: WinFsp Offline Pin Fix** — Resolve deployment-blocking File Explorer hang on Windows when navigating offline-pinned mounts
- [x] **Phase 2: Observability Infrastructure** — Build the event bus, error accumulator, stat methods, and Tauri commands that power the dashboard (completed 2026-03-18)
- [ ] **Phase 3: Dashboard UI** — Deliver the full observability surface: drive status, activity feed, error log, cache usage, offline pin health
- [ ] **Phase 4: Visual Polish & Hardening** — Modernize the UI, ensure consistent feedback, and validate cross-platform parity

## Phase Details

### Phase 1: WinFsp Offline Pin Fix
**Goal**: Users can navigate offline-pinned mounts in File Explorer without crashes or hangs
**Depends on**: Nothing (first phase, deployment blocker)
**Requirements**: BUG-01
**Success Criteria** (what must be TRUE):
  1. User on Windows can pin a folder for offline access, disconnect from the network, and browse the pinned folder in File Explorer without any hang or crash
  2. File Explorer responds within 2-3 seconds when navigating any folder in an offline-pinned mount (no 30s timeout stalls)
  3. User who reconnects after offline period sees the mount resume normal sync without requiring remount or app restart
**Plans:** 2 plans
Plans:
- [x] 01-01-PLAN.md — VFS-path Graph API timeout + log rotation
- [x] 01-02-PLAN.md — Memory cache eviction protection + SQLite metadata population during pin

### Phase 2: Observability Infrastructure
**Goal**: All sync state, activity, errors, and cache metrics are queryable from the backend — the data layer is complete and testable before any UI work
**Depends on**: Phase 1 (VFS behavior patterns from bug fix inform event design)
**Requirements**: (none — enabling infrastructure for Phase 3)
**Success Criteria** (what must be TRUE):
  1. A Tauri `invoke("get_dashboard_status")` call returns per-drive sync state, online/offline status, last synced timestamp, and auth health — verifiable from the browser console
  2. A Tauri `invoke("get_recent_errors")` call returns the most recent errors with file name, error type, and timestamp — verifiable from the browser console
  3. A Tauri `invoke("get_cache_stats")` call returns disk cache usage (bytes used vs. configured max), pinned item count, and writeback queue contents — verifiable from the browser console
  4. Real-time events (sync progress, state transitions, new errors) are pushed to the frontend via Tauri `emit()` — verifiable by subscribing with `listen()` in the browser console
**Plans:** 4/4 plans complete
Plans:
- [x] 02-01-PLAN.md — Core observability types (ObsEvent enum, response structs) + cache stat methods
- [x] 02-02-PLAN.md — Observability module (ring buffers, event bridge) + AppState extensions
- [x] 02-03-PLAN.md — Tauri dashboard commands + delta sync/VFS event wiring
- [ ] 02-04-PLAN.md — Browser console verification checkpoint

### Phase 3: Dashboard UI
**Goal**: Users see sync state, activity, errors, cache usage, and offline status at a glance — the app is transparent, not a black box
**Depends on**: Phase 2 (backend data layer must be queryable)
**Requirements**: DASH-01, DASH-02, DASH-03, DASH-04, DASH-05, ACT-01, ACT-02, ACT-03, ACT-04, ACT-05, COFF-01, COFF-02
**Success Criteria** (what must be TRUE):
  1. User opens the app and immediately sees a dashboard with per-drive status cards showing online/offline state, sync status ("Up to date" / "Syncing N files" / "Error"), and last synced time
  2. User can see a scrollable activity feed of recent sync events (uploads, downloads, deletes, conflicts) and an error log panel with actionable detail (file name, error type, timestamp)
  3. User can see cache disk usage as a visual bar ("2.1 GB / 5 GB"), upload queue counts, writeback queue file names, and per-pin offline health status ("Downloaded" / "Partial" / "Stale")
  4. User sees a degraded auth banner when token refresh is failing, and conflict notifications surface directly in the dashboard (not buried in logs)
  5. Dashboard updates in near-real-time (~1-2s) as sync events occur, without requiring manual refresh
**Plans**: TBD

### Phase 4: Visual Polish & Hardening
**Goal**: The UI looks professional and every user action provides visible feedback — ready for org-wide deployment
**Depends on**: Phase 3 (functionality must be complete before polish)
**Requirements**: UI-01, UI-02
**Success Criteria** (what must be TRUE):
  1. UI has consistent styling — proper spacing, aligned elements, semantic color palette for status indicators (online/offline/syncing/error), and a professional, modern appearance
  2. Every user-initiated action (mount, unmount, sync now, pin folder) shows visible feedback via status indicators — no operation completes silently
  3. Dashboard renders correctly on both Linux (FUSE) and Windows (WinFsp) without layout breaks or missing data
**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. WinFsp Offline Pin Fix | 2/2 | Complete | 2026-03-18 |
| 2. Observability Infrastructure | 4/4 | Complete   | 2026-03-18 |
| 3. Dashboard UI | 0/? | Not started | — |
| 4. Visual Polish & Hardening | 0/? | Not started | — |

---
*Roadmap created: 2026-03-18*
*Last updated: 2026-03-18*
