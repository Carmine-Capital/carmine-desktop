---
run: run-cloud-mount-023
work_item: fix-vfs-ux
intent: fix-comprehensive-review
mode: confirm
checkpoint: plan
approved_at: pending
---

# Implementation Plan: Conflict notification+naming, statfs quota, error mapping, copy blocking

## Approach

Six targeted fixes to VFS user experience, all in the shared `CoreOps` layer (affecting both FUSE and CfApi backends):

1. **Conflict notification** — Add `tokio::sync::mpsc::UnboundedSender<VfsEvent>` to `CoreOps`. When `flush_inode` detects a conflict and successfully uploads a conflict copy, it sends a `VfsEvent::ConflictDetected` event. The app layer creates the channel, spawns a receiver task, and calls `notify::conflict_detected()`.

2. **Conflict file naming** — Fix `flush_inode` to insert `.conflict.{timestamp}` before the final extension. `report.docx` → `report.conflict.1741...docx` (preserves extension for OS association).

3. **statfs quota** — Add `GraphClient::get_drive()` method (hits `GET /drives/{drive_id}`, returns `Drive` with quota). Cache quota in `CoreOps` behind a `Mutex<Option<(Instant, DriveQuota)>>` with 60s TTL. `statfs` calls `CoreOps::get_quota()` which returns cached or fresh values; falls back to large-space on failure.

4. **Error mapping** — Add `PermissionDenied`, `TimedOut`, `QuotaExceeded` variants to `VfsError`. Add `VfsError::from_core_error()` helper that inspects `cloudmount_core::Error` variants (GraphApi status 403→PermissionDenied, 404→NotFound, Network→TimedOut, quota→QuotaExceeded). Map to specific errno in `fuse_fs.rs` (EACCES, ETIMEDOUT, ENOSPC) and CfApi error kinds.

5. **Rename conflict check** — Before deleting the destination in `rename()`, check if it's a remote file with different content (different eTag or has pending writes). If so, save it as a conflict copy (same naming as #2) before proceeding.

6. **Copy timeout reduction** — Reduce `COPY_MAX_POLL_DURATION_SECS` from 300→10. If server-side copy times out, fall back to `copy_file_range_fallback` (read/write through buffers) instead of returning an error. Use `tokio::time::sleep` instead of `std::thread::sleep`.

## Files to Create

| File | Purpose |
|------|---------|
| (none) | |

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-vfs/src/core_ops.rs` | Add VfsEvent enum + sender field to CoreOps, fix conflict naming, add quota cache + get_quota(), add VfsError variants + from_core_error(), rename conflict check, reduce copy timeout + fallback |
| `crates/cloudmount-vfs/src/fuse_fs.rs` | Map new VfsError variants to errno (EACCES, ETIMEDOUT, ENOSPC), update statfs to call CoreOps::get_quota() |
| `crates/cloudmount-vfs/src/cfapi.rs` | Map new VfsError variants to CfApi error kinds |
| `crates/cloudmount-graph/src/client.rs` | Add `get_drive(drive_id)` method |
| `crates/cloudmount-app/src/notify.rs` | Add `conflict_detected()` notification function |
| `crates/cloudmount-app/src/main.rs` | Create VfsEvent channel, pass sender to mount constructors, spawn receiver task |
| `crates/cloudmount-vfs/src/mount.rs` | Update CloudMountFs constructor to accept event sender |

## Tests

| Test File | Coverage |
|-----------|----------|
| `crates/cloudmount-graph/tests/graph_tests.rs` | `get_drive()` with wiremock — quota populated, quota missing |
| `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | Conflict naming helper (extension preservation, no-extension files) |

## Technical Details

### Conflict naming algorithm
```rust
fn conflict_name(original: &str, timestamp: i64) -> String {
    match original.rfind('.') {
        Some(pos) => {
            let (stem, ext) = original.split_at(pos);
            format!("{stem}.conflict.{timestamp}{ext}")
        }
        None => format!("{original}.conflict.{timestamp}"),
    }
}
```

### Quota cache
```rust
struct QuotaCache {
    data: std::sync::Mutex<Option<(std::time::Instant, DriveQuota)>>,
}
```
TTL: 60 seconds. On failure: return `None` (caller uses large fallback values). Thread-safe via `std::sync::Mutex` (held only briefly for cache check).

### VfsError additions
```rust
pub enum VfsError {
    NotFound,
    NotADirectory,
    DirectoryNotEmpty,
    PermissionDenied,
    TimedOut,
    QuotaExceeded,
    IoError(String),
}
```

### Copy timeout fallback
When `COPY_MAX_POLL_DURATION_SECS` (now 10s) expires, `copy_file_range_server` returns a specific error that `copy_file_range` catches and redirects to `copy_file_range_fallback`.

---
*Plan approved at checkpoint. Execution follows.*
