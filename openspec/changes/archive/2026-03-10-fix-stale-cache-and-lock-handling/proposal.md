## Why

Two bugs and a missing capability: (1) files updated on OneDrive appear corrupted when opened locally before delta sync runs, because the disk cache serves stale-but-internally-consistent content without checking the server; (2) upload failures (e.g. 423 Locked when a file is being co-authored online) are silently swallowed on Linux/FUSE with no user notification; (3) there is no handling for OneDrive file locks — users can edit a locked file locally and only discover the problem when the upload fails.

## What Changes

- Move the server metadata refresh (`get_item`) in `open_file` before the disk cache validation so stale content is never served from cache
- Add `VfsEvent::UploadFailed` variant and emit it from the FUSE `flush` callback (parity with CfApi's `WritebackFailed` on flush errors)
- Detect 423 Locked in `flush_inode`: instead of returning a generic `IoError`, upload a conflict copy (using existing `conflict_name()` pattern) and notify the user
- Check file lock status via Graph API on `open_file` and emit a warning notification when the file is locked online

## Capabilities

### New Capabilities
- `file-lock-handling`: Detection and handling of OneDrive file locks — warn on open, save as conflict copy on 423

### Modified Capabilities
- `fuse-stale-read-prevention`: Fix disk cache bypass — server metadata refresh must run before disk cache validation, not after
- `ui-feedback`: Add notification for upload failures and file lock warnings
- `virtual-filesystem`: Add conflict copy behavior for 423 Locked uploads

## Impact

- `cloudmount-vfs/src/core_ops.rs`: Reorder `open_file` logic, add lock check, add 423 handling in `flush_inode`, new `VfsEvent` variant
- `cloudmount-vfs/src/fuse_fs.rs`: Emit `VfsEvent::UploadFailed` from `flush` callback
- `cloudmount-graph/src/client.rs`: Expose 423 as a distinct error variant (like `PreconditionFailed` for 412)
- `cloudmount-core/src/error.rs`: New `Error::Locked` variant
- `cloudmount-app/src/main.rs`: Handle new `VfsEvent` variants in event forwarder
- `cloudmount-app/src/notify.rs`: New notification functions for upload failure and lock warning
