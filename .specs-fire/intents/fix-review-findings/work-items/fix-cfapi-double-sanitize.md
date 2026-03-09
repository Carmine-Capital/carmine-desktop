---
id: fix-cfapi-double-sanitize
title: Remove duplicate drive_id sanitization in main.rs
intent: fix-review-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-09T17:00:00Z
run_id: run-cloud-mount-011
completed_at: 2026-03-09T17:09:29.573Z
---

# Work Item: Remove duplicate drive_id sanitization in main.rs

## Description

main.rs:798 calls `drive_id.replace('!', "_")` before passing to `CfMountHandle::mount`, but cfapi.rs:431 (`build_sync_root_id`) already performs the same replacement. Remove the caller-side sanitization so `build_sync_root_id` is the single owner of this concern.

## Acceptance Criteria

- [ ] main.rs no longer calls `replace('!', "_")` on drive_id before passing to CfMountHandle
- [ ] build_sync_root_id in cfapi.rs still sanitizes `!` to `_`
- [ ] cargo check and cargo clippy pass clean with -Dwarnings

## Technical Notes

The replacement is idempotent so current behavior is correct — this is a code quality cleanup to establish a single sanitization owner.

## Dependencies

(none)
