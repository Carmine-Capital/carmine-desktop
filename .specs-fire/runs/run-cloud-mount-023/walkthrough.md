# Walkthrough: fix-vfs-ux

**Run**: run-cloud-mount-023 | **Work Item**: fix-vfs-ux | **Mode**: confirm

## Summary

Six targeted fixes to VFS user experience: conflict notifications reach the desktop, conflict files preserve their extension, `statfs` reports real quota, error codes are specific instead of generic `EIO`, rename checks for conflicts before overwriting, and server-side copy no longer blocks FUSE for up to five minutes.

## Changes

### 1. Conflict Notification (`core_ops.rs`, `main.rs`, `notify.rs`)

Added a `VfsEvent` enum with a `ConflictDetected { path: String }` variant. `CoreOps` now holds an `Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>`. When `flush_inode` detects an eTag mismatch and successfully uploads a conflict copy, it sends a `VfsEvent::ConflictDetected` through the channel.

In `main.rs`, `start_mount` creates the unbounded channel and passes the sender to the `CoreOps` constructor (threaded through `mount.rs` and `MountHandle::mount`). A `tokio::spawn` receiver task listens for events and calls `notify::conflict_detected()`, which sends a desktop notification with the conflicting filename.

### 2. Conflict File Naming (`core_ops.rs`)

New `conflict_name()` function splits the original filename at its last `.` to preserve the extension. `report.docx` becomes `report.conflict.1741...docx` instead of the previous `report.docx.conflict.1741...`. Files without an extension append `.conflict.{timestamp}` directly. Used in both `flush_inode` (write conflict) and the new rename conflict check.

### 3. statfs Quota (`client.rs`, `core_ops.rs`, `fuse_fs.rs`)

Added `GraphClient::get_drive(drive_id)` which hits `GET /drives/{drive_id}` and returns the `Drive` struct including optional quota fields (`total`, `remaining`, `used`).

`CoreOps` gains a quota cache: `Mutex<Option<(Instant, DriveQuota)>>` with 60-second TTL. The `get_quota()` method returns cached values if fresh, otherwise calls `get_drive()` and updates the cache. On any failure, returns `None`.

`fuse_fs.rs` `statfs` now calls `get_quota()`. When quota data is available, it computes `blocks`, `bfree`, and `bavail` from real values. When unavailable, falls back to the previous large-space constants.

### 4. Error Mapping (`core_ops.rs`, `fuse_fs.rs`)

Added three variants to `VfsError`: `PermissionDenied`, `TimedOut`, and `QuotaExceeded`. New `VfsError::from_core_error()` inspects `cloudmount_core::Error` variants:

- Graph API 403 status -> `PermissionDenied`
- Graph API 404 status -> `NotFound`
- Network/timeout errors -> `TimedOut`
- Quota exceeded -> `QuotaExceeded`
- Everything else -> `IoError(msg)`

In `fuse_fs.rs`, the errno mapping extends: `PermissionDenied` -> `EACCES`, `TimedOut` -> `ETIMEDOUT`, `QuotaExceeded` -> `ENOSPC`. This gives users and tools meaningful error codes instead of blanket `EIO`.

### 5. Rename Conflict Check (`core_ops.rs`)

Before `rename()` deletes a destination file to make way for the source, it now calls `has_server_conflict()`. This checks whether the destination has a different eTag than what the local inode table knows, or has pending writes. If a conflict is detected, the existing destination is downloaded and re-uploaded under a conflict name (using `conflict_name()`) before proceeding with the rename. This prevents silent data loss when overwriting a file that was modified server-side.

### 6. Copy Timeout Reduction (`core_ops.rs`)

`COPY_MAX_POLL_DURATION_SECS` reduced from 300 to 10. When server-side copy polling exceeds this duration, the function now returns `VfsError::TimedOut` instead of a generic error. In `copy_file_range`, this specific error is caught and triggers `copy_file_range_fallback` (a read/write copy through local buffers) rather than surfacing the error to the user.

Replaced `std::thread::sleep` with `tokio::time::sleep` in the polling loop, which is correct for async code running inside `rt.block_on`.

## Files Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-graph/src/client.rs` | Added `get_drive()` method for drive quota endpoint |
| `crates/cloudmount-vfs/src/core_ops.rs` | `VfsEvent` enum, `conflict_name()`, `VfsError` variants, `from_core_error()`, quota cache, `get_quota()`, `has_server_conflict()`, rename conflict check, copy timeout + fallback, `tokio::time::sleep` |
| `crates/cloudmount-vfs/src/fuse_fs.rs` | New errno mappings (EACCES, ETIMEDOUT, ENOSPC), real quota in `statfs`, `event_tx` parameter |
| `crates/cloudmount-vfs/src/mount.rs` | Thread `event_tx` through `MountHandle::mount` |
| `crates/cloudmount-app/src/notify.rs` | `conflict_detected()` notification function |
| `crates/cloudmount-app/src/main.rs` | Event channel creation, receiver task spawn, pass `event_tx` to mount |
| `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | 4 conflict naming tests |
| `crates/cloudmount-graph/tests/graph_tests.rs` | 2 `get_drive` wiremock tests |
| `crates/cloudmount-vfs/tests/fuse_integration.rs` | Updated `MountHandle::mount` calls with `None` event_tx |
| `crates/cloudmount-app/tests/integration_tests.rs` | Updated `MountHandle::mount` calls with `None` event_tx |

## Decisions

| Decision | Rationale |
|----------|-----------|
| Unbounded channel for VfsEvent | Conflicts are rare; bounded channel risks deadlocking synchronous FUSE callbacks that use `rt.block_on` |
| `std::sync::Mutex` for quota cache | Lock held only for instant comparison + option swap; no async work under lock |
| 10s copy poll timeout | Long enough for most small-to-medium server-side copies; short enough to avoid perceptible FUSE stalls |
| Fallback to read/write copy on timeout | Ensures copy always completes from the user's perspective, even if server-side copy is slow |
| `rfind('.')` for extension splitting | Correct for the common case (single extension); for double extensions like `.tar.gz`, preserves `.gz` which is the one the OS uses for file association |
| `Option<UnboundedSender>` in CoreOps | Allows test code and non-desktop builds to pass `None`, avoiding channel setup when notifications are not needed |
