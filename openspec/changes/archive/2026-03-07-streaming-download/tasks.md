## 1. Graph client streaming download

- [x] 1.1 Add `download_streaming()` method to `GraphClient` in `crates/cloudmount-graph/src/client.rs`: send `GET /drives/{driveId}/items/{itemId}/content` with auth header, return `impl Stream<Item = Result<Bytes>>` via reqwest's `.bytes_stream()` (do NOT collect the full body). Include token acquisition and error handling matching existing `download_content()` pattern.
- [x] 1.2 Add unit test for `download_streaming()` using wiremock: mock a large response body, verify chunks arrive progressively via the stream, verify auth header is sent.

## 2. DownloadProgress and StreamingBuffer types

- [x] 2.1 Define `DownloadProgress` enum in `core_ops.rs`: `InProgress(u64)`, `Done`, `Failed(String)`.
- [x] 2.2 Define `StreamingBuffer` struct in `core_ops.rs`: `data: tokio::sync::RwLock<Vec<u8>>`, `progress: watch::Sender<DownloadProgress>`, `progress_rx: watch::Receiver<DownloadProgress>`, `total_size: u64`. Add constructor that pre-allocates `data` to `total_size` and initializes progress to `InProgress(0)`.
- [x] 2.3 Implement `StreamingBuffer::append_chunk(&self, chunk: &[u8])`: acquire write lock on `data`, copy chunk bytes into the pre-allocated buffer at the current position, update progress via watch sender to `InProgress(new_position)`.
- [x] 2.4 Implement `StreamingBuffer::mark_done(&self)` and `StreamingBuffer::mark_failed(&self, msg: String)`: update watch sender to `Done` or `Failed(msg)`.
- [x] 2.5 Implement `StreamingBuffer::wait_for_range(&self, offset: u64, size: u64, rt: &Handle) -> VfsResult<()>`: subscribe to progress watch, loop waiting until either (a) downloaded bytes >= offset + size, (b) progress is `Done`, or (c) progress is `Failed` (return `VfsError::IoError`). Use `rt.block_on(rx.changed())` to block the calling FUSE thread.
- [x] 2.6 Implement `StreamingBuffer::read_range(&self, offset: usize, size: usize) -> Vec<u8>`: acquire read lock on `data`, slice `[offset..min(offset+size, downloaded)]` and return.
- [x] 2.7 Implement `StreamingBuffer::downloaded_bytes(&self) -> u64`: read current progress from the watch receiver without blocking.
- [x] 2.8 Add unit tests for `StreamingBuffer`: append chunks updates progress, `wait_for_range` blocks until sufficient data, `wait_for_range` returns error on `Failed`, `read_range` returns correct slices.

## 3. DownloadState enum and OpenFile refactor

- [x] 3.1 Define `DownloadState` enum in `core_ops.rs`: `Complete(Vec<u8>)`, `Streaming { buffer: Arc<StreamingBuffer>, task: tokio::task::AbortHandle }`.
- [x] 3.2 Replace `OpenFile.content: Vec<u8>` with `content: DownloadState`. Update `OpenFile.dirty` to only be settable when state is `Complete`.
- [x] 3.3 Update `OpenFileTable::insert()` to accept `DownloadState` instead of `Vec<u8>`.
- [x] 3.4 Add helper methods on `DownloadState`: `is_complete() -> bool`, `as_complete(&self) -> Option<&Vec<u8>>`, `as_complete_mut(&mut self) -> Option<&mut Vec<u8>>`, `into_complete(self) -> Option<Vec<u8>>`.

## 4. CoreOps open_file streaming path

- [x] 4.1 Refactor `CoreOps::open_file()`: after checking writeback/disk cache (which return `Complete` state as before), check if the file size (from metadata) is below `SMALL_FILE_LIMIT` — if so, download fully via `download_content()` and insert as `Complete`. If size >= `SMALL_FILE_LIMIT`, create a `StreamingBuffer`, spawn a background download task, and insert as `Streaming`.
- [x] 4.2 Implement the background download task: call `graph.download_streaming()`, iterate over the byte stream appending chunks to `StreamingBuffer`, call `mark_done()` on completion, call `mark_failed()` on error. After `mark_done()`, write the full buffer to disk cache via `cache.disk.put()`.
- [x] 4.3 Import `SMALL_FILE_LIMIT` from `cloudmount_graph` (or re-define locally in `core_ops.rs`) for the size threshold check. Ensure the constant is accessible cross-crate or duplicated with a comment referencing the source.

## 5. CoreOps read_handle streaming support

- [x] 5.1 Update `CoreOps::read_handle()` for `DownloadState::Complete`: no change from current behavior — slice from `Vec<u8>`.
- [x] 5.2 Update `CoreOps::read_handle()` for `DownloadState::Streaming`: check if requested range `[offset..offset+size]` is within downloaded bytes. If yes, call `StreamingBuffer::read_range()` and return. If the offset is within 2 MB of the download frontier, call `StreamingBuffer::wait_for_range()` then `read_range()`. If the offset is more than 2 MB ahead, issue an on-demand `graph.download_range()` call via `rt.block_on()` and return those bytes directly.
- [x] 5.3 Define `RANDOM_ACCESS_THRESHOLD: u64 = 2 * 1024 * 1024` constant in `core_ops.rs` for the sequential-vs-random-access decision boundary.

## 6. CoreOps write_handle streaming support

- [x] 6.1 Update `CoreOps::write_handle()`: if `DownloadState::Streaming`, block until download completes by calling `StreamingBuffer::wait_for_range(0, total_size)`, then transition state from `Streaming` to `Complete` by extracting the finished buffer. Proceed with normal in-place write on the `Complete` buffer.
- [x] 6.2 Handle the case where `wait_for_range` returns `Failed`: return `VfsError::IoError` to the write caller.

## 7. CoreOps release_file cancellation

- [x] 7.1 Update `CoreOps::release_file()`: if `DownloadState::Streaming`, call `task.abort()` on the `AbortHandle` to cancel the background download task before dropping the `OpenFile` entry.
- [x] 7.2 Ensure the `Arc<StreamingBuffer>` is dropped when both the task and the `OpenFile` entry are gone (no manual cleanup needed — Arc handles this).

## 8. CoreOps truncate and flush integration

- [x] 8.1 Update `CoreOps::truncate()`: if the open file is in `Streaming` state, wait for download to complete before truncating (same pattern as write — block until `Done`, transition to `Complete`, then truncate).
- [x] 8.2 Update `CoreOps::flush_handle()`: if `Streaming`, wait for download to complete and transition to `Complete` before flushing to writeback and uploading.

## 9. FUSE backend adjustments

- [x] 9.1 Review `fuse_fs.rs::open()`: no code change needed — it already delegates to `CoreOps::open_file()` which now returns immediately for large files.
- [x] 9.2 Review `fuse_fs.rs::read()`: no code change needed — `CoreOps::read_handle()` handles the blocking internally. The FUSE thread pool tolerates blocking reads.
- [x] 9.3 Review `fuse_fs.rs::release()`: no code change needed — `CoreOps::release_file()` handles cancellation internally.

## 10. CfApi backend adjustments

- [x] 10.1 Update `cfapi.rs::fetch_data()`: for uncached files, call `graph.download_range()` directly for the `required_file_range` instead of going through `open_file()` + `read_handle()` + `release_file()`. Keep the existing path for cached files (open/read/release via CoreOps).
- [x] 10.2 Add a `CoreOps::read_range_direct()` helper that checks disk cache first, falls back to `download_range()` for the exact requested region. Use this from `cfapi.rs::fetch_data()`.

## 11. Tests

- [x] 11.1 Add unit tests for `DownloadState` transitions: `Complete` reads work, `Streaming` blocks until data available, `Streaming` transitions to `Complete` after done, `Streaming` returns error after failure.
- [x] 11.2 Add integration test for streaming open/read lifecycle: mock a large file response in wiremock, open file (should return immediately), read first chunk (should block briefly then return), read second chunk (should return data), release file.
- [x] 11.3 Add integration test for cancellation: open a large file, read a small portion, release the handle before download completes, verify no disk cache entry is created.
- [x] 11.4 Add integration test for random-access read: open a large file, read from an offset far beyond the download frontier, verify that a range request is issued (check wiremock received a Range header), verify correct bytes returned.
- [x] 11.5 Add integration test for download failure propagation: mock a response that errors mid-stream, verify read returns `VfsError::IoError`.
- [x] 11.6 Add integration test for write-to-streaming-file: open a large uncached file, issue a write, verify it blocks until download completes, verify the write is applied correctly.
- [x] 11.7 Verify existing tests still pass — run `cargo test --all-targets` and `cargo clippy --all-targets` to ensure no regressions or warnings.
