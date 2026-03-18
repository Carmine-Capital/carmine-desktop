# Requirements: CarmineDesktop — Stabilization & Observability

**Defined:** 2026-03-18
**Core Value:** When something goes wrong, you know about it and can diagnose it — the app is transparent, not a black box.

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### Bug Fix

- [x] **BUG-01**: WinFsp offline pin crash is resolved — File Explorer no longer hangs when navigating a mounted drive with pinned folders after losing network connectivity

### Dashboard

- [ ] **DASH-01**: User can see a dashboard view showing sync state, activity, errors, cache usage, and offline status at a glance
- [ ] **DASH-02**: User can see per-drive sync status indicator showing "Up to date" / "Syncing N files" / "Error" for each mounted drive
- [ ] **DASH-03**: User can see online/offline status indicator per drive, prominently displayed
- [ ] **DASH-04**: User can see auth status indicator — degraded auth banner displayed when token refresh is failing or re-authentication is required
- [ ] **DASH-05**: User can see last synced timestamp per drive ("Last synced: 2 minutes ago") that updates in real time

### Activity & Errors

- [ ] **ACT-01**: User can see upload queue count showing number of files uploading and number queued ("3 uploading, 2 queued")
- [ ] **ACT-02**: User can see recent errors in the UI with actionable detail: file name, error type, timestamp, and enough context to understand what went wrong
- [ ] **ACT-03**: User can see conflict notifications surfaced in the UI — which files had conflicts, when, and that a conflict copy was created
- [ ] **ACT-04**: User can see a recent activity feed showing synced, uploaded, and deleted items in a scrollable log
- [ ] **ACT-05**: User can see writeback queue detail showing which specific files are pending upload, by name (not just count)

### Cache & Offline

- [ ] **COFF-01**: User can see cache disk usage display showing current usage vs. configured maximum (e.g., "2.1 GB / 5 GB")
- [ ] **COFF-02**: User can see offline pin health status per pin: "Downloaded" / "Partial" / "Stale" — indicating whether pinned content is actually available offline

### UI Polish

- [ ] **UI-01**: UI visual design is modernized with consistent styling, proper spacing, and professional appearance
- [ ] **UI-02**: All user-facing actions provide visible feedback via status indicators — no operation completes silently

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Dashboard Enhancements

- **DASH-06**: User can trigger manual sync per drive via a "Sync Now" button
- **DASH-07**: User can see per-file upload progress for large files during upload
- **DASH-08**: User can see sync metrics over time (latency, throughput, error rate) in a mini chart

### Diagnostics

- **DIAG-01**: User can export a diagnostic report bundling logs, config, cache stats, and sync metrics for IT support
- **DIAG-02**: User can configure bandwidth throttling for upload/download operations

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Per-file sync status overlay icons | VFS model (FUSE/WinFsp) doesn't integrate with shell icon overlay providers. Dashboard is the observability surface. |
| Real-time file-change streaming to UI | Per-operation push is noisy and complex. Poll-based SyncMetrics with batched event delivery is sufficient. |
| Selective sync / folder exclusion | VFS architecture means files are on-demand by nature — selective sync doesn't apply. |
| Version history / file restore UI | Available via "Open in SharePoint" (existing command). Building in-app would require significant Graph API work. |
| Multi-account switching | Organizational M365 only for v1. Single account display. |
| Dark mode / theme customization | Out of scope for stabilization. Respect system theme via `prefers-color-scheme` if trivial. |
| Pause/resume sync | Defer to future milestone. |
| Notification center UI | OS notification center handles this. Dashboard error panel for persistent errors. |
| New cloud providers | OneDrive/SharePoint only for v1. |
| Mobile app | Desktop-only product. |
| Personal Microsoft accounts | Organizational M365 only. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| BUG-01 | Phase 1: WinFsp Offline Pin Fix | Complete |
| DASH-01 | Phase 3: Dashboard UI | Pending |
| DASH-02 | Phase 3: Dashboard UI | Pending |
| DASH-03 | Phase 3: Dashboard UI | Pending |
| DASH-04 | Phase 3: Dashboard UI | Pending |
| DASH-05 | Phase 3: Dashboard UI | Pending |
| ACT-01 | Phase 3: Dashboard UI | Pending |
| ACT-02 | Phase 3: Dashboard UI | Pending |
| ACT-03 | Phase 3: Dashboard UI | Pending |
| ACT-04 | Phase 3: Dashboard UI | Pending |
| ACT-05 | Phase 3: Dashboard UI | Pending |
| COFF-01 | Phase 3: Dashboard UI | Pending |
| COFF-02 | Phase 3: Dashboard UI | Pending |
| UI-01 | Phase 4: Visual Polish & Hardening | Pending |
| UI-02 | Phase 4: Visual Polish & Hardening | Pending |

**Coverage:**
- v1 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-18*
*Last updated: 2026-03-18 after roadmap creation*
