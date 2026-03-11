## Context

The WinFsp backend (`winfsp_fs.rs`) was implemented as part of the `replace-cfapi-with-winfsp` change. It delegates all business logic to `CoreOps` (shared with FUSE), but its `FileSystemContext` trait implementation is missing the `flush` callback. The trait's default `flush` returns `STATUS_INVALID_DEVICE_REQUEST`, which causes any application calling `FlushFileBuffers()` — notably Microsoft Excel — to receive an error.

This produces two user-visible symptoms:
1. **Can't save**: Excel writes content, calls `FlushFileBuffers()`, gets an error, reports save failure. The data IS flushed later in `cleanup`, but Excel has already given up.
2. **False corruption**: When opening a remotely-updated file, Excel creates a lock file (`~$Book1.xlsx`), writes to it, then flushes — the flush error may cause Excel to distrust the filesystem and report the target file as corrupted (repair works because the content is actually fine).

A secondary issue: the `open` method captures a `DriveItem` from `resolve_path` before calling `open_file()`, which refreshes the memory cache. The returned `FileInfo` uses stale timestamps from the pre-refresh item.

Additionally, every Office file open uploads a `~$*.xlsx` lock file to OneDrive then deletes it on close — two wasted Graph API calls per file open.

## Goals / Non-Goals

**Goals:**
- Fix the missing `flush` callback so `FlushFileBuffers()` succeeds
- Fix the stale `DriveItem` used for `FileInfo` in `open`
- Skip uploads for known transient files (Office lock files, Windows system files)

**Non-Goals:**
- Rewriting the WinFsp backend architecture
- Adding a user-configurable exclude list (hardcoded patterns are sufficient for v1)
- Filtering temp files from directory listings (they should still appear locally)
- Handling `.tmp` files generically (too broad — only well-known patterns)

## Decisions

### 1. `flush` delegates to `CoreOps::flush_handle`

Same pattern as the FUSE backend (`fuse_fs.rs:424`). The `flush` callback receives the file context, extracts the file handle, calls `ops.flush_handle(fh)`, and updates the returned `FileInfo`.

**Alternative considered**: Making `flush` a no-op that returns `Ok(())`. This would unblock `FlushFileBuffers()` but defer all upload work to `cleanup`. Rejected because it breaks the contract — applications expect `flush` to persist data, and a silent no-op could cause data loss if the process crashes between flush and cleanup.

### 2. Re-fetch item after `open_file` for accurate `FileInfo`

In `open`, after calling `self.ops.open_file(ino)`, re-read the item from the memory cache via `self.ops.lookup_item(ino)` instead of using the stale `item` captured before the call. This ensures timestamps and other metadata in the returned `FileInfo` match the refreshed server state.

### 3. Temp-file filter in `flush_inode` with filename pattern matching

The filter lives in `CoreOps::flush_inode`, not in the WinFsp or FUSE layer. This means both backends benefit. The check is a simple function `is_transient_file(name: &str) -> bool` that matches:

- `~$*` — Office lock files
- `~*.tmp` — Office temp files
- `Thumbs.db` — Windows thumbnail cache
- `desktop.ini` — Windows folder settings
- `.DS_Store` — macOS directory metadata

When a file matches, `flush_inode` skips the upload and removes the writeback entry (the content is transient and should not persist to the server). The file continues to exist in the local VFS (create/write/read/delete all work normally through the in-memory buffer).

**Alternative considered**: Filtering at the `create` level (never create these files at all). Rejected because applications expect `create` to succeed — denying it would break Office's lock file mechanism. The file must exist locally; it just shouldn't be uploaded.

**Alternative considered**: Filtering in the WinFsp/FUSE layer before calling `CoreOps`. Rejected because the filter should apply uniformly regardless of backend — putting it in `CoreOps` is the single correct location.

## Risks / Trade-offs

- **[Risk] False positive on temp filter** — A user file legitimately named `~$report.xlsx` would be silently skipped. → Mitigation: The `~$` prefix is reserved by Office and not a realistic user filename. Other patterns (`Thumbs.db`, `desktop.ini`) are system files that should never be on OneDrive via a VFS mount.

- **[Risk] Future temp patterns missed** — New applications may create different temp file patterns. → Mitigation: The pattern list is centralized in one function (`is_transient_file`) and easy to extend. Log a debug message when a file is skipped so it's diagnosable.

- **[Risk] Stale item fix changes timing** — Re-reading from memory cache after `open_file` adds one DashMap lookup. → Mitigation: Negligible cost — this is an in-memory concurrent hashmap read, not a network call.
