---
id: fix-review-findings
title: Fix Review Findings
status: completed
created: 2026-03-09T17:00:00Z
completed_at: 2026-03-09T17:36:44.945Z
---

# Intent: Fix Review Findings

## Goal

Fix all issues surfaced by the cross-platform and UX reviews of the current working tree: 1 cross-platform finding (double sanitization) and 18 UX findings (silent failures, missing accessibility, minor polish).

## Users

End users of CloudMount (error feedback, accessibility) and developers (code quality, single-owner sanitization).

## Problem

Users encounter silent failures with no feedback on sign-in errors, mount removal, settings load, and clipboard copy. Keyboard and screen-reader users are blocked from navigating tabs, selecting sites/libraries, and hearing status announcements. Sanitization logic is duplicated across main.rs and cfapi.rs.

## Success Criteria

- All mutating actions show success/error feedback to the user
- Keyboard users can navigate tabs, sites, and libraries
- Screen readers announce errors and status changes via ARIA live regions
- Sanitization logic has a single owner (build_sync_root_id)
- No regressions: cargo clippy and cargo check pass clean

## Constraints

- CSP script-src 'self' — no inline handlers
- No build step — vanilla JS only
- RUSTFLAGS=-Dwarnings — zero warnings

## Notes

Findings sourced from two parallel review agents:
- Cross-platform reviewer: 1 finding (double sanitization of ! in drive_id)
- UX reviewer: 18 findings (B1, S1-S6, D1-D4, A1-A5, M1-M7)
