## Why

Copying a file within the mount (e.g., `cp bigfile.docx bigfile-backup.docx`) downloads the entire file and re-uploads it, even though the Microsoft Graph API supports server-side copy via `POST /drives/{driveId}/items/{itemId}/copy` with zero data transfer. A 500 MB file copy currently moves 1 GB over the network; with server-side copy it moves zero bytes.

## What Changes

- Add a `copy_item()` method to `GraphClient` that POSTs to the Graph API copy endpoint and returns a monitor URL for the async server-side operation.
- Add a `poll_copy_status()` method to `GraphClient` that polls the monitor URL until the copy completes, fails, or times out.
- Add `CopyMonitorResponse` type to `carminedesktop-core` for deserializing the monitor URL JSON responses.
- Add `copy_file_range()` to `CoreOps` with detection logic: use server-side copy when both source and destination are remote items and the copy covers the full file; fall back to in-memory buffer copy otherwise.
- Implement the FUSE `copy_file_range` trait method in `carminedesktopFs`, delegating to `CoreOps`.
- After a successful server-side copy, reassign the destination inode from its temporary `local:` ID to the real server item ID and update all caches.

## Capabilities

### New Capabilities

_(none -- this extends existing Graph client and VFS capabilities)_

### Modified Capabilities

- `graph-client`: Add server-side copy endpoint (`POST /drives/{driveId}/items/{itemId}/copy`) and async copy status polling via the monitor URL.
- `virtual-filesystem`: Add `copy_file_range` FUSE operation with server-side copy optimization for full-file remote-to-remote copies, with fallback to buffer-level copy for partial or local copies.

## Impact

- **Code**: `crates/carminedesktop-graph/src/client.rs` (new methods), `crates/carminedesktop-core/src/types.rs` (new type), `crates/carminedesktop-vfs/src/core_ops.rs` (new method), `crates/carminedesktop-vfs/src/fuse_fs.rs` (new trait impl).
- **Tests**: New integration tests for the Graph copy endpoint (wiremock), unit tests for eligibility detection and fallback logic.
- **Dependencies**: None added. Uses existing `reqwest`, `serde`, `fuser`.
- **Backwards compatibility**: Additive change. Existing read+write copy path remains as the fallback. No config or API changes.
- **Platform scope**: FUSE only (Linux/macOS). Windows CfApi has no copy callback; copies on Windows continue via existing fetch_data/closed callbacks.
