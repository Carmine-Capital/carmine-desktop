---
id: fix-vfs-ux
title: Conflict notification+naming, statfs quota, error mapping, copy blocking
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: pending
depends_on: [fix-vfs-data-safety]
created: 2026-03-09T18:00:00Z
---

# Work Item: Conflict notification+naming, statfs quota, error mapping, copy blocking

## Description

Fix VFS user experience issues:

1. **No conflict notification** (`core_ops.rs:651`): Conflicts create `.conflict.*` files with only a log warning. Fix: add a notification channel (e.g., `tokio::sync::mpsc`) from CoreOps to the app layer. App sends desktop notification via `notify::conflict_detected()`.

2. **Conflict file naming** (`core_ops.rs:658`): `report.docx.conflict.174...` — extension lost. Fix: use `report.conflict.174....docx` pattern — insert `.conflict.{timestamp}` before the final extension.

3. **statfs fake quota** (`fuse_fs.rs:435`): Reports ~512TB free. Fix: query `graph.get_drive_quota()` (add Graph API call for `/me/drive` quota endpoint), cache the result with TTL, report actual available space.

4. **Generic I/O errors** (`fuse_fs.rs`, `core_ops.rs`): Network errors surface as `EIO` with no detail. Fix: map common error types to more specific errno values (EACCES for 403, ENOENT for 404, ETIMEDOUT for timeout, ENOSPC for quota exceeded).

5. **Rename silent deletion** (`core_ops.rs`): Rename to existing destination silently deletes remote file. Fix: check if destination exists and has different content; if so, create conflict copy before overwriting.

6. **Server-side copy blocks FUSE** (`core_ops.rs:27`): `COPY_MAX_POLL_DURATION_SECS=300` blocks FUSE thread for up to 5 minutes. Fix: spawn copy polling on a separate Tokio task; return to FUSE immediately with a "pending" state, or reduce max poll time and return EINPROGRESS.

## Acceptance Criteria

- [ ] Desktop users get notification when conflict detected
- [ ] Conflict files preserve the original file extension
- [ ] statfs reports actual OneDrive quota (or reasonable fallback on failure)
- [ ] Network errors map to specific errno values, not just EIO
- [ ] Rename to existing file checks for conflicts before overwriting
- [ ] Server-side copy does not block FUSE thread for more than a few seconds
- [ ] New Graph API quota endpoint has tests with wiremock

## Technical Notes

For conflict naming, split `item.name` on the last `.` to get `(stem, ext)`. Then `format!("{stem}.conflict.{timestamp}.{ext}")`. Handle files with no extension.

For statfs, the Graph API endpoint is `GET /me/drive` which returns `quota.remaining`, `quota.total`, `quota.used`. Cache with 60s TTL to avoid per-statfs API calls.

## Dependencies

- fix-vfs-data-safety (conflict handling must be fixed first)
