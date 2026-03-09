---
id: fix-comprehensive-review
title: Fix Comprehensive Review Findings
status: in_progress
created: 2026-03-09T18:00:00Z
---

# Intent: Fix Comprehensive Review Findings

## Goal

Fix 66 verified issues discovered during comprehensive cross-platform (4 reviewers) and UX (3 reviewers) review of the entire CloudMount codebase. Issues span CRIT/HIGH/MED/LOW across all 6 crates and the frontend.

## Users

CloudMount end-users on Linux, macOS, and Windows. Developers maintaining the codebase.

## Problem

The review uncovered 6 CRITICAL data-loss paths (conflict upload silently ignored, unmount flush wrong params, StreamingBuffer unbounded RAM, crash recovery discards files, case-insensitive lookup missing on Windows, hardcoded errno wrong on macOS), 2 HIGH logic bugs (set_interval no-op, CI clippy Linux-only), 22 MEDIUM robustness/UX gaps (auth security, tray dead items, frontend error handling, accessibility), and 36 LOW/INFO polish items. Left unfixed, users risk silent data loss, broken Windows experience, and degraded UX.

## Success Criteria

- Zero CRITICAL data-loss paths remain
- Cross-platform: builds and lints on all 3 platforms in CI with desktop feature
- Frontend: all actions give user feedback, keyboard-navigable, no raw Rust errors shown
- Auth: token files have restrictive permissions, try_restore uses correct account_id
- All existing tests pass, no new warnings (RUSTFLAGS=-Dwarnings)

## Constraints

- Rust 2024, MSRV 1.85, zero warnings
- Frontend: CSP script-src 'self', no inline handlers, vanilla JS
- Changes must not break existing mount/unmount lifecycle
- 3 items filtered as NOISE (keyring backends, large file upload, config corruption) — do not fix

## Notes

Review was conducted by 7 parallel agents: 4 cross-platform reviewers (VFS, core+config+auth, graph+cache, app Rust) and 3 UX reviewers (HTML, JS, CSS). Findings were consolidated, then verified by 3 additional agents that read actual source code. 66 of 69 claims confirmed REAL, 3 confirmed NOISE.
