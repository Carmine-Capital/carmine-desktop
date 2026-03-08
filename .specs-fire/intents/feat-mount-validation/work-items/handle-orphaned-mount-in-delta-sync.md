---
id: handle-orphaned-mount-in-delta-sync
title: Handle orphaned mount detection in delta sync
intent: feat-mount-validation
complexity: medium
mode: confirm
status: completed
depends_on:
  - validate-mount-before-start
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-005
completed_at: 2026-03-08T13:59:52.788Z
---

# Work Item: Handle orphaned mount detection in delta sync

## Description

During `run_delta_sync`, if the Graph API returns 404 on the delta endpoint (library deleted mid-session), detect this as an orphaned mount, stop the mount, remove it from config, and notify the user. This covers the case where a library is deleted after the initial mount validation at startup.

## Acceptance Criteria

- [ ] `run_delta_sync` (or its caller in `main.rs`) distinguishes 404 from other errors
- [ ] On 404 during delta sync: mount is stopped, removed from `user_config`, notification sent: "SharePoint library 'X' was deleted or moved and has been removed from your configuration"
- [ ] On 403 during delta sync: sync paused for that mount (not removed), notification sent once: "Lost access to 'X' — check permissions"
- [ ] Other errors: existing behavior (logged, retried on next cycle)
- [ ] The delta sync loop in `start_delta_sync` handles per-mount errors without stopping other mounts
- [ ] `cargo test -p cloudmount-cache` passes (update sync tests if needed)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings

## Technical Notes

- `run_delta_sync` in `cloudmount-cache/src/sync.rs` returns `cloudmount_core::Result<()>` — add a new `Error` variant `DriveNotFound` or `DriveAccessDenied` to distinguish 404/403
- The caller `start_delta_sync` in `main.rs` loops per mount — it can match on these new variants and trigger cleanup
- Cleanup from sync context: needs access to `AppState` to mutate `user_config` — pass `AppHandle` or a callback into the sync loop
- Notification deduplication: 403 should not spam the user every sync cycle — track "already notified" state per mount (e.g., a `HashSet<mount_id>` in `AppState`)

## Dependencies

- validate-mount-before-start
