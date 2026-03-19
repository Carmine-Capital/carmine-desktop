---
phase: 4
slug: visual-polish-hardening
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-19
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Manual visual inspection (CSS/JS frontend changes) |
| **Config file** | none — no visual regression tooling in stack |
| **Quick run command** | `make build` |
| **Full suite command** | `make clippy && make test` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `make build`
- **After every plan wave:** Run `make clippy && make test`
- **Before `/gsd:verify-work`:** Full suite must be green + manual visual inspection
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | UI-01 | manual-only | Manual: verify token values in `:root` match UI-SPEC | N/A | ⬜ pending |
| 04-01-02 | 01 | 1 | UI-01 | manual-only | Manual: verify typography scale consolidation | N/A | ⬜ pending |
| 04-01-03 | 01 | 1 | UI-01 | manual-only | Manual: verify component styles (buttons, inputs, toggles, badges) | N/A | ⬜ pending |
| 04-01-04 | 01 | 1 | UI-01 | manual-only | Manual: verify spacing/whitespace changes | N/A | ⬜ pending |
| 04-02-01 | 02 | 2 | UI-01 | manual-only | Manual: verify inline styles migrated to CSS classes | N/A | ⬜ pending |
| 04-02-02 | 02 | 2 | UI-02 | manual-only | Manual: trigger removeSource() and verify showStatus() fires | N/A | ⬜ pending |
| 04-02-03 | 02 | 2 | UI-02 | manual-only | Manual: verify Copy button text consistency | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No new test files needed — Phase 4 is CSS/JS only. CI (`make clippy`, `make test`) validates no Rust code was accidentally broken.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual design modernized — soft dark palette, typography, surfaces, spacing | UI-01 | CSS rendering changes require human visual inspection; no visual regression framework in stack | Open settings.html and wizard.html in Tauri webview on host; verify palette is lighter, typography is consolidated, card borders are softer, spacing is consistent |
| All actions provide feedback | UI-02 | `showStatus()` calls require triggering UI actions and observing status bar | Trigger mount, unmount, sync now, pin folder, copy URL, remove source; verify each shows status feedback |
| Cross-platform rendering | UI-01 | Layout validation on different OS requires running on each platform | Run on Linux (FUSE) and Windows (WinFsp); verify no layout breaks or missing data |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: `make build` after every commit
- [x] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [x] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
