---
id: validate-mount-before-start
title: Validate mount resource exists before starting
intent: feat-mount-validation
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-005
completed_at: 2026-03-08T13:55:46.348Z
---

# Work Item: Validate mount resource exists before starting

## Description

Before mounting each drive, call `GET /drives/{drive_id}` to verify the resource still exists. On 404, remove the mount from config and notify the user. On 403, skip the mount (keep config) and notify. On network/transient errors, skip with a warning and retry on next sync cycle.

## Acceptance Criteria

- [ ] `start_mount` (or a new `validate_mount` helper called from it) performs `GET /drives/{drive_id}` before mounting
- [ ] 404 response: mount removed from `user_config` + notification sent via `notify` module: "SharePoint library 'X' is no longer accessible and has been removed from your configuration"
- [ ] 403 response: mount skipped, config unchanged, notification: "No access to 'X' — check your permissions"
- [ ] Transient errors (network timeout, 5xx): mount skipped with `tracing::warn!`, config unchanged, no notification
- [ ] Validation uses a single attempt (no retry) — 404/403 are definitive, not transient
- [ ] Startup time not significantly impacted (validations run concurrently per mount or sequentially with minimal overhead)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings

## Technical Notes

- `GET /drives/{drive_id}` can reuse the existing `get_my_drive` pattern in `cloudmount-graph/src/client.rs` — add a `get_drive(drive_id: &str)` method or reuse `get_json`
- Error classification: check HTTP status code from `GraphError` or `reqwest::Response`
- `start_mount` in `main.rs` is currently sync (calls `rt.block_on` for async VFS ops) — validation should be async, called before the blocking section
- New notification helpers needed in `notify.rs`: `mount_not_found(app, name)` and `mount_access_denied(app, name)`

## Dependencies

(none)
