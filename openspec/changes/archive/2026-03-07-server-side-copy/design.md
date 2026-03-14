## Context

The VFS layer handles file copies through the standard open/read/write/flush path. When a user runs `cp bigfile.docx bigfile-backup.docx` within the mount, the kernel calls `open` on the source (downloading the full file), `create` on the destination, then issues a series of `copy_file_range` syscalls (Linux 4.5+). Since `carminedesktopFs` does not implement `copy_file_range`, fuser returns `ENOSYS`, and the kernel falls back to `read` + `write` pairs. This downloads the entire file from OneDrive/SharePoint into memory and re-uploads it via the writeback+flush path.

The Microsoft Graph API supports server-side copy via `POST /drives/{driveId}/items/{itemId}/copy`. This operation runs entirely on the server with no data transfer to or from the client. It returns HTTP 202 with a `Location` header pointing to a monitor URL. The client polls the monitor URL until the copy completes, at which point it receives the new item's `resourceId`.

The FUSE `copy_file_range` callback receives both source and destination file handles, offsets, and a length. When conditions allow server-side copy (full-file, both items remote), we can short-circuit the entire data path.

## Goals / Non-Goals

**Goals:**
- Implement server-side copy for full-file remote-to-remote copies within the mount, eliminating network data transfer
- Provide a buffer-level fallback for partial copies, local-to-remote copies, and platforms without `copy_file_range`
- Add Graph API copy endpoint support (`copy_item`, `poll_copy_status`) to `GraphClient`
- Handle the async nature of the Graph copy operation with polling and timeout
- Properly update all caches and inode mappings after server-side copy completes

**Non-Goals:**
- Cross-drive copies (the VFS currently mounts a single drive; the Graph API supports cross-drive copies but there is no use case until multi-drive mounting is implemented)
- Server-side copy for partial file ranges (the Graph API only supports full-item copy)
- Windows CfApi copy optimization (CfApi has no copy callback; copies go through existing fetch_data/closed)
- macOS copy optimization (macFUSE/FUSE-T do not support `copy_file_range`; `cp` falls back to read+write)
- Folder copy (FUSE does not invoke `copy_file_range` for directories; `cp -r` recurses with individual file copies)

## Decisions

### D1: Server-side copy eligibility detection in CoreOps

The `CoreOps::copy_file_range()` method checks eligibility before attempting server-side copy. The conditions are:

1. Source item_id does NOT start with `local:` (it is a real remote item with a server-side ID)
2. `offset_in == 0` (copy starts from the beginning of the source file)
3. `len >= source_file_size` (the copy covers the full file content)
4. Source item has metadata available (needed for the parent reference of the destination)

When any condition fails, the method falls back to reading from the source handle's buffer and writing into the destination handle's buffer. This is a pure in-memory operation since both files are already open.

**Why these conditions:** The Graph API `copy` endpoint copies an entire item; it does not support byte-range copies. The `local:` prefix indicates a file that has not yet been uploaded to the server and therefore has no server-side item to copy from. The offset and length checks ensure we are being asked for a full-file copy, not a splice or partial range.

**Alternative considered:** Detecting copy intent at the `create` level (e.g., if the kernel passes `O_COPY` flags). This is not available in the FUSE protocol; `copy_file_range` is the only signal.

### D2: Destination handling — reassign local ID after server-side copy

When `copy_file_range` is called, the destination file has already been created via `create()` with a temporary `local:{nanos}` item ID. After the server-side copy completes:

1. Fetch the newly created item metadata using the `resourceId` from the monitor response
2. Call `InodeTable::reassign()` to map the destination inode from `local:{nanos}` to the real server item ID
3. Update the memory cache with the full `DriveItem` metadata from the server
4. Remove any writeback buffer entry for the old `local:` ID (it was created empty by `create()`)
5. Update the open file handle's content buffer with the actual file content size (mark as non-dirty since the server already has the data)

**Why reassign:** The inode was allocated during `create()` and must persist through the copy. The kernel expects the same inode number. `reassign` updates the bidirectional mapping without changing the inode number.

### D3: Graph API copy method returns monitor URL, separate polling method

The `GraphClient` gets two new methods:

- `copy_item(drive_id, item_id, dest_drive_id, dest_parent_id, dest_name) -> Result<String>`: POSTs to the copy endpoint, expects HTTP 202, extracts the `Location` header as the monitor URL. Uses `with_retry` for the initial POST. Returns the monitor URL string.

- `poll_copy_status(monitor_url) -> Result<CopyStatus>`: GETs the monitor URL (no auth header needed — the URL is pre-authenticated). Returns a `CopyStatus` enum: `InProgress { percentage: f64 }`, `Completed { resource_id: String }`, or `Failed { message: String }`.

**Why separate methods:** The initial copy request and the polling loop have different retry semantics. The copy POST should retry on 429/5xx (transient errors). The poll GET has its own retry logic (exponential backoff between polls, max duration timeout). Separating them keeps each method focused and testable.

**Why no auth on monitor URL:** The Graph API monitor URL is a pre-authenticated, time-limited URL. Including an `Authorization` header is unnecessary and the URL is independent of the Graph API base URL.

### D4: Polling strategy — exponential backoff with absolute timeout

The polling loop in `CoreOps::copy_file_range`:

1. Initial delay: 500ms
2. Backoff factor: 2x per iteration, capped at 5s
3. Absolute timeout: 300s (5 minutes) from the start of polling
4. On transient HTTP errors during poll: retry up to 3 times per poll attempt before considering the copy failed
5. On `CopyStatus::Failed`: log error, return `VfsError::IoError`
6. On timeout: log warning, return `VfsError::IoError`

**Why 5 minutes:** Server-side copy of large files (multi-GB) can take several minutes. The 5-minute timeout is generous enough for files up to ~50 GB based on Microsoft's documented copy performance, while preventing indefinite blocking of the FUSE thread.

**Trade-off:** The FUSE `copy_file_range` callback is synchronous and blocks the calling thread. A long-running poll ties up one of fuser's worker threads. This is acceptable because: (a) the alternative (read+write) would block even longer for large files due to data transfer, and (b) fuser uses a thread pool so other operations continue.

### D5: Buffer-level fallback — read from source handle, write to destination handle

When server-side copy is not eligible, the fallback copies data between the two open file handles' in-memory buffers:

1. Read `len` bytes from the source handle's buffer at `offset_in`
2. Write those bytes into the destination handle's buffer at `offset_out`
3. Mark the destination handle as dirty
4. Return the number of bytes copied

This is purely in-memory (no network I/O) since both files are already open with content loaded. It is strictly better than returning `ENOSYS` (which forces the kernel to do individual `read` + `write` calls through FUSE) because it avoids per-call FUSE overhead.

**Why not return ENOSYS:** Returning `ENOSYS` from `copy_file_range` causes the kernel to fall back to `read` + `write` pairs, each of which crosses the FUSE kernel-userspace boundary. The buffer-level fallback copies data in a single call with no boundary crossings.

### D6: CopyMonitorResponse type in carminedesktop-core

A new `CopyMonitorResponse` struct in `types.rs`:

```
CopyMonitorResponse {
    status: String,                    // "inProgress", "completed", "failed", etc.
    percentage_complete: Option<f64>,  // 0.0 - 100.0
    resource_id: Option<String>,       // present on completion
    error: Option<GraphErrorBody>,     // present on failure (reuse existing type)
}
```

And a `CopyStatus` enum in the graph crate for the parsed result:

```
CopyStatus::InProgress { percentage: f64 }
CopyStatus::Completed { resource_id: String }
CopyStatus::Failed { message: String }
```

**Why in carminedesktop-core:** `CopyMonitorResponse` is a Graph API JSON shape, consistent with other Graph response types (`DriveItem`, `DeltaResponse`, `UploadSession`) that live in `types.rs`. The `CopyStatus` enum lives in the graph crate as it is the parsed, domain-specific representation used by `poll_copy_status`.

## Risks / Trade-offs

**[FUSE thread blocking]** The synchronous `copy_file_range` callback blocks a fuser worker thread for the duration of the server-side copy poll loop (up to 5 minutes). **Mitigation:** fuser uses a thread pool; other operations continue on other threads. The alternative (read+write) would block even longer for large files. For extremely large files where server copy exceeds 5 minutes, the timeout fires and returns an error — the user can retry or use a different copy method.

**[Race condition with delta sync]** A delta sync cycle running concurrently with a server-side copy could see the new item appear before `CoreOps` has finished updating its local state (inode table, caches). **Mitigation:** Delta sync already handles new items by allocating inodes via `InodeTable::allocate`, which is idempotent — if the item ID is already assigned to an inode, it returns the existing inode. The `reassign` in `copy_file_range` runs before delta sync would process the item, so the inode mapping is consistent.

**[Monitor URL expiration]** The monitor URL returned by the Graph API is time-limited. If polling is delayed (e.g., system suspend), the URL may expire. **Mitigation:** If a poll returns an HTTP error, the copy is treated as failed and an error is returned to FUSE. The user's `cp` command fails and can be retried.

**[Partial server-side copy on failure]** If the server-side copy fails midway, a partial item may exist on the server. **Mitigation:** The Graph API copy is atomic from the client's perspective — the monitor reports either `completed` or `failed`. On `failed`, no item is created. If an item was created but the status endpoint reports failure, it is the server's responsibility to clean up.

**[Destination file already open with empty buffer]** The `create()` call opens the destination file with an empty buffer. After server-side copy, the destination buffer should reflect the copied content size, but loading the full content into the buffer would negate the zero-transfer benefit. **Mitigation:** After server-side copy, update the open file handle's content buffer to the correct size (populated lazily or from disk cache on next read). Mark the handle as non-dirty since the server already has the data. If the user immediately reads the copied file, the `read_handle` call will serve from the buffer (which may need to be re-populated from disk cache or network — but this is the same as opening any cached file).
