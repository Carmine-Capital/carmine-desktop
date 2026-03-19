---
phase: 3
slug: dashboard-ui
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-18
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Manual browser console verification (WebView) |
| **Config file** | None — frontend has no automated test framework |
| **Quick run command** | `make build` |
| **Full suite command** | `make clippy && make build` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `make build`
- **After every plan wave:** Run `make clippy && make build` + manual visual verification
- **Before `/gsd:verify-work`:** Full manual walkthrough of all 12 requirements in running app
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | DASH-01 | manual-only | Visual: open settings window, verify dashboard is default panel | N/A | ⬜ pending |
| 03-01-02 | 01 | 1 | DASH-02 | manual-only | Console: `invoke('get_dashboard_status')` then verify cards show correct text | N/A | ⬜ pending |
| 03-01-03 | 01 | 1 | DASH-03 | manual-only | Toggle network, verify dot changes color | N/A | ⬜ pending |
| 03-01-04 | 01 | 1 | DASH-04 | manual-only | Console: verify banner appears when `authDegraded: true` | N/A | ⬜ pending |
| 03-01-05 | 01 | 1 | DASH-05 | manual-only | Trigger sync, verify timestamp changes | N/A | ⬜ pending |
| 03-01-06 | 01 | 1 | ACT-01 | manual-only | Write file to mount, verify "1 uploading" appears | N/A | ⬜ pending |
| 03-01-07 | 01 | 1 | ACT-02 | manual-only | Trigger conflict/error, verify error row with actionable hint | N/A | ⬜ pending |
| 03-01-08 | 01 | 1 | ACT-03 | manual-only | Trigger conflict, verify amber-bordered entry in errors | N/A | ⬜ pending |
| 03-01-09 | 01 | 1 | ACT-04 | manual-only | Trigger sync, verify activity entries appear | N/A | ⬜ pending |
| 03-01-10 | 01 | 1 | ACT-05 | manual-only | Expand upload queue disclosure, verify file names listed | N/A | ⬜ pending |
| 03-01-11 | 01 | 1 | COFF-01 | manual-only | Verify bar shows correct usage; cache some files and re-check | N/A | ⬜ pending |
| 03-01-12 | 01 | 1 | COFF-02 | manual-only | Pin folder, verify health badge on dashboard + Offline panel | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.* No test framework installation needed — this is a manual-only frontend verification phase. `make build` validates static assets are bundled correctly.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Dashboard panel visible on window open | DASH-01 | Pure UI — WebView visual state | Open settings window, verify dashboard is the active default panel |
| Per-drive sync status text | DASH-02 | Requires running app with connected drives | Console: `invoke('get_dashboard_status')`, verify cards show correct sync text |
| Online/offline status dot | DASH-03 | Network state visual indicator | Toggle network connectivity, verify dot color changes |
| Auth degraded banner | DASH-04 | Auth state visual indicator | Force auth failure, verify warning banner appears with "Sign In" action |
| Last synced timestamp updates | DASH-05 | Time-dependent visual update | Trigger sync, verify timestamp refreshes |
| Upload queue count | ACT-01 | Requires active upload | Write file to mount, verify upload count displays |
| Error entries with detail | ACT-02 | Requires triggering errors | Trigger conflict/error, verify error row with file name, type, timestamp, hint |
| Conflict amber border | ACT-03 | Visual styling verification | Trigger conflict, verify amber left border distinct from red errors |
| Activity feed entries | ACT-04 | Requires sync events | Trigger sync operations, verify activity feed populates |
| Writeback queue file names | ACT-05 | Requires active writeback | Expand upload queue disclosure, verify file names |
| Cache usage bar | COFF-01 | Visual progress bar | Verify bar shows correct usage with color thresholds |
| Pin health badges | COFF-02 | Requires pinned folders | Pin folder, verify health badge shows correct status |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
