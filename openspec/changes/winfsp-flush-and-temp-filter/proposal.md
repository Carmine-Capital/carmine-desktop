## Why

The WinFsp backend is missing the `flush` trait callback, causing two user-visible bugs: Excel (and any application calling `FlushFileBuffers()`) cannot save files to the mounted drive, and opening remotely-updated files triggers false corruption warnings because Excel's pre-open flush of the lock file fails. Additionally, every Office file open generates two unnecessary Graph API calls (upload + delete) for temporary lock files (`~$*.xlsx`) that should never reach the server.

## What Changes

- Implement the `flush` callback on `FileSystemContext` for the WinFsp backend, delegating to `CoreOps::flush_handle()` — same as the FUSE backend's existing `flush` implementation
- Fix a stale-metadata bug in `winfsp_fs::open` where `item_to_file_info` uses a pre-refresh `DriveItem` for timestamps (the `item` variable is captured before `open_file()` refreshes the memory cache)
- Add a temp-file upload filter in `flush_inode` that skips the upload cycle for files matching known transient patterns (Office lock files `~$*`, Windows system files like `Thumbs.db`, `desktop.ini`, and common temp patterns), while still allowing local create/write/delete to work normally through the in-memory buffer

## Capabilities

### New Capabilities
- `temp-file-upload-filter`: Heuristic in the upload pipeline that skips Graph API uploads for files matching known transient/system filename patterns, reducing unnecessary network traffic and server-side churn

### Modified Capabilities

## Impact

- `crates/cloudmount-vfs/src/winfsp_fs.rs` — add `flush` method to `FileSystemContext` impl, fix stale item in `open`
- `crates/cloudmount-vfs/src/core_ops.rs` — add temp-file name check in `flush_inode` before upload
- No dependency changes, no API changes, no breaking changes
