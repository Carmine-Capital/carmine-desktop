---
id: feat-mount-validation
title: Mount validation before start and on delta sync
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T13:59:52.795Z
---

# Intent: Mount validation before start and on delta sync

## Goal

Validate each mount against the Graph API before starting it, and detect orphaned mounts during delta sync. Handle deleted libraries and revoked access gracefully with user-facing notifications and automatic config cleanup.

## Users

All CloudMount users — especially those whose SharePoint library access changes (library deleted, permissions revoked, library moved/renamed).

## Problem

Currently:
- `start_mount` starts every configured mount without verifying the resource still exists
- If a SharePoint library is deleted, `run_delta_sync` returns 404 → error is logged but the mount stays in config and keeps failing on every sync cycle
- If access is revoked (403), same behavior — silent repeated failure
- Users see confusing errors with no actionable feedback and no way to auto-clean stale mounts

## Success Criteria

- Before starting a mount, a lightweight Graph call verifies the drive exists
- 404 response → mount removed from config + user notification: "SharePoint library 'X' is no longer accessible and has been removed"
- 403 response → mount skipped (not removed) + user notification: "No access to 'X' — check permissions"
- Network/transient error → mount skipped with retry on next sync, config unchanged
- During delta sync, 404 on `get_delta` → same cleanup flow as above
- Notifications use existing `notify` module pattern
- CI passes (clippy + fmt)

## Constraints

- Validation call should be lightweight (metadata only, not full delta)
- Use existing Graph client retry/backoff — validation is one attempt only (don't retry a 404)
- Must not block startup significantly — validation runs per-mount before mounting
- Depends on: none (can be applied independently, but pairs well with `feat-account-scoped-mounts`)

## Notes

This is Layer 2 of the ultimate solution. The validation call can reuse an existing Graph endpoint (`GET /drives/{drive_id}`) which is already used in `get_my_drive`. Delta sync orphan detection needs a new error variant or a way to distinguish 404 (resource gone) from other Graph errors in `run_delta_sync`.
