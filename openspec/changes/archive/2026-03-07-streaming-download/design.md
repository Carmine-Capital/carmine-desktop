## Context

The VFS layer currently loads the entire file content into memory on `open()` before returning a file handle (see `CoreOps::open_file` at `core_ops.rs:508`). This was an explicit simplicity-first decision (D3 in the open-file-table design) with a documented plan to add streaming later.

The problem is acute for large files: opening a 500 MB video over a typical 50 Mbps connection blocks for ~80 seconds. During that time the FUSE thread is stuck in `rt.block_on(graph.download_content(...))`, making the mount point unresponsive to other operations on that thread.

The Graph client already has two download methods: `download_content()` (full file, used everywhere) and `download_range()` (partial via HTTP Range header, implemented but never called). The `SMALL_FILE_LIMIT` constant (4 MB) already exists as the threshold for simple vs. session uploads.

Key constraints:
- FUSE `Filesystem` trait methods are sync — blocking a FUSE thread on download progress is acceptable (FUSE uses a thread pool).
- CfApi's `fetch_data` callback already receives a `required_file_range`, making it naturally suited to range-based fetching.
- `tokio` with "full" features is already a dependency, providing `sync::watch`, `sync::Notify`, and `task::JoinHandle`.
- No new external dependencies are needed.

## Goals / Non-Goals

**Goals:**
- Eliminate blocking on `open()` for large uncached remote files — return the file handle immediately and download in the background.
- Allow `read()` to return data as soon as the requested byte range is available, without waiting for the full download to complete.
- Support sequential read-ahead (video playback, file copy) so the download stays ahead of the reader.
- Support random-access reads (seeking in a video) via on-demand range requests using `download_range()`.
- Cancel in-flight downloads when `release()` is called before the download finishes.
- Propagate download errors to any reader blocked waiting for bytes.
- Populate the disk cache once the full download completes, so subsequent opens are instant.
- Maintain full backward compatibility for the write path (writes still require full content).

**Non-Goals:**
- Partial disk cache (storing incomplete downloads across restarts) — files must fully download before being persisted to disk cache.
- Memory-mapped I/O or zero-copy read paths.
- Adaptive chunk sizing based on network speed.
- Parallel multi-connection downloads for a single file.
- Changes to the upload path or writeback buffer.
- FUSE `max_read` tuning or kernel-level read-ahead configuration (separate concern).

## Decisions

### D1: Hybrid strategy — eager for small, streaming for large

Files smaller than `SMALL_FILE_LIMIT` (4 MB) continue to download fully on `open()`, exactly as today. Files at or above 4 MB use the new streaming path.

**Why 4 MB:** This constant already exists and represents the threshold where download latency becomes noticeable (~0.6s on 50 Mbps). Small files benefit from the simplicity of eager loading. The threshold also aligns with the simple-vs-session upload boundary, keeping the mental model consistent.

**Alternative considered:** Always stream regardless of size. Rejected because the streaming machinery (spawn task, watch channel, progress tracking) adds overhead that is wasted for files that download in under a second.

### D2: DownloadState enum inside OpenFile

Replace `OpenFile.content: Vec<u8>` with a `content: DownloadState` enum:

```
enum DownloadState {
    /// Content fully available (small file, cached file, or download complete).
    Complete(Vec<u8>),
    /// Background download in progress.
    Streaming {
        buffer: Arc<StreamingBuffer>,
        task: AbortHandle,
    },
}
```

`StreamingBuffer` wraps:
- `data: RwLock<Vec<u8>>` — the growing byte buffer, pre-allocated to the known file size.
- `downloaded: watch::Sender<DownloadProgress>` — broadcasts progress to waiting readers.
- `total_size: u64` — the expected file size from metadata.

`DownloadProgress` is:
```
enum DownloadProgress {
    /// Bytes available so far.
    InProgress(u64),
    /// Download completed successfully.
    Done,
    /// Download failed with an error message.
    Failed(String),
}
```

**Why Arc<StreamingBuffer>:** The buffer is shared between the background download task (writer) and the FUSE read path (reader). `Arc` gives both sides ownership. `RwLock` allows concurrent reads while the download task holds a brief write lock to append each chunk.

**Why watch channel:** `tokio::sync::watch` is a single-producer, multi-consumer channel that always holds the latest value. Readers can efficiently wait for a specific progress threshold without polling. Multiple concurrent readers (e.g., two processes reading the same open file handle's buffer — though each handle has its own buffer) are supported naturally.

**Alternative considered:** `tokio::sync::Notify` — simpler but doesn't carry a value; readers would need to separately check the buffer length after each notification, creating a TOCTOU gap under concurrent access. `watch` eliminates this by carrying the progress value atomically.

### D3: Background download task with chunked streaming

When `open_file()` determines a file needs streaming (large + not cached), it:
1. Pre-allocates `StreamingBuffer.data` to the file's known size (from metadata).
2. Spawns a Tokio task that calls `graph.download_content()` but uses reqwest's streaming response body (`.bytes_stream()`) to process chunks as they arrive.
3. Each chunk is appended to the buffer and the watch channel is updated with the new byte count.
4. On completion, the task transitions the watch to `Done` and writes the full content to disk cache.
5. On error, the task transitions the watch to `Failed(msg)`.

**Why not download_range in a loop:** Sequential chunked download of the full file via a single HTTP connection is more efficient than issuing many independent range requests. Range requests are reserved for random-access reads (D4). The Graph API's CDN handles streaming well via a single GET with no Range header.

**Implementation:** A new `GraphClient::download_streaming()` method returns a `impl Stream<Item = Result<Bytes>>` (using reqwest's `.bytes_stream()`). The `reqwest` crate already has the `stream` feature enabled in workspace dependencies. This avoids loading the entire response into memory before returning.

### D4: On-demand range requests for random access

When `read_handle()` is called on a `Streaming` buffer and the requested range `[offset..offset+size]` is not yet downloaded:

1. If `offset` is within 2 MB of the current download frontier (sequential or near-sequential access), block and wait for the background download to reach the needed offset.
2. If `offset` is more than 2 MB ahead of the download frontier (random access / seek), issue an on-demand `download_range()` call for exactly the requested region, return those bytes, and let the background download continue independently.

**Why 2 MB threshold:** Balances latency (at 50 Mbps, 2 MB downloads in ~0.3s — acceptable wait) against unnecessary range requests. A seek 100 MB ahead should not wait for the sequential download to catch up.

**Why not cancel-and-restart:** Cancelling the background download and restarting from the new offset would lose the already-downloaded prefix. Many seek patterns (video player seeking back, then resuming playback) benefit from having the prefix available.

**Alternative considered:** Always use range requests, never do background download. Rejected because sequential workloads (video playback from the start, `cp` command) would issue thousands of small range requests, each with its own HTTP overhead. A single streaming connection is far more efficient.

### D5: Cancellation on release

When `release_file()` is called and the `OpenFile` has `DownloadState::Streaming`, the `AbortHandle` is used to cancel the background Tokio task. This is safe because:
- The `AbortHandle` from `tokio::task::spawn` + `JoinHandle::abort_handle()` cancels the task at the next `.await` point.
- The `StreamingBuffer` is dropped when both the task (via `Arc`) and the `OpenFile` drop their references.
- The disk cache is NOT populated for cancelled downloads (incomplete data).

**Edge case — dirty streaming file:** If a file was opened, partially downloaded, written to (dirty flag set), and then released: the write path already works on `OpenFile.content`. To write to a streaming file, the write must first wait for the download to complete (or fail), then transition to `Complete` state. This preserves the existing write-path invariant that content is fully available.

### D6: Error propagation to blocked readers

When the background download fails (network error, auth expiry, server error):
1. The watch channel is updated to `Failed(error_message)`.
2. Any reader blocked in `read_handle()` waiting for progress wakes up, sees `Failed`, and returns `VfsError::IoError`.
3. Subsequent reads on the same handle also return `VfsError::IoError` (the error state is sticky).
4. The FUSE layer translates this to `Errno::EIO` as it does for all `VfsError::IoError`.

### D7: CfApi integration

CfApi's `fetch_data()` already receives a `required_file_range`. Instead of opening the full file and reading the range, the updated implementation:
1. Checks the disk cache — if the file is fully cached, serve from there (no change).
2. For uncached files, calls `download_range()` directly for the requested range, bypassing the streaming machinery entirely.

**Why bypass streaming for CfApi:** CfApi inherently operates on ranges — the OS tells us exactly which bytes it needs. There is no benefit to spawning a background download; the OS will call `fetch_data` again for the next range it needs. This is simpler and more aligned with the CfApi model.

### D8: Write-to-streaming-file semantics

If a `write_handle()` is called on an `OpenFile` in `Streaming` state:
1. Block until the download completes (wait for `Done` on the watch channel).
2. Transition the `DownloadState` from `Streaming { buffer, task }` to `Complete(data)` by extracting the completed buffer.
3. Proceed with the normal in-place write on the `Complete` buffer.

**Why block on write:** Writes need the full buffer to splice data at an arbitrary offset. Allowing writes to a partially-downloaded buffer would require tracking which regions are "real data" vs. "zero-filled placeholder," adding significant complexity for a rare case (writing to a file that is still downloading).

**Trade-off:** Writing to a large file that is still downloading will block until the download finishes. This is acceptable because: (a) it matches the pre-streaming behavior (open blocked anyway), and (b) the common case is read-only access to large files (video, documents).

## Risks / Trade-offs

**[Memory: pre-allocation for large files]** The streaming buffer pre-allocates to the full file size on open. A user opening a 2 GB file allocates 2 GB immediately even though only the first few MB may be read. **Mitigation:** This matches the current behavior (which downloads the whole file into memory). Future work could use a sparse buffer or memory-mapped file, but that is out of scope.

**[Complexity in OpenFile]** The `DownloadState` enum adds two code paths (Complete vs. Streaming) to every read/write/release operation. **Mitigation:** The `Complete` path is identical to today's code — the streaming path is an additive overlay. Helper methods on `DownloadState` (`.wait_for_range()`, `.is_complete()`) encapsulate the complexity.

**[Race between download completion and release]** The background task and the release call may race: the task is completing (writing to disk cache) while release is aborting it. **Mitigation:** The disk cache write is the last thing the task does after setting `Done`. If aborted before that point, no disk cache entry is created (safe). If the task already set `Done` and is writing to disk cache, the abort will cancel it mid-write, but `disk.put()` is atomic (write-then-rename), so a partial write is harmless.

**[Range request overhead for random access]** Each random-access read issues a separate HTTP Range request to the Graph API. Rapid seeking (scrubbing a video timeline) could generate many requests. **Mitigation:** FUSE read sizes are typically 128 KB-1 MB. The 2 MB threshold means only genuine seeks (not sequential reads that are slightly behind) trigger range requests. Rate limiting at the Graph API level is handled by existing retry/backoff logic.

**[Auth expiry during long downloads]** A large file download may outlast the access token's lifetime (typically 1 hour). **Mitigation:** `download_content`/`download_range` go through `with_retry` which calls `self.token()` on each attempt. For the streaming path, the initial connection is established once, and HTTP streaming does not re-authenticate mid-stream. If the connection drops due to token expiry, the retry logic in the streaming download task will obtain a fresh token on retry.
