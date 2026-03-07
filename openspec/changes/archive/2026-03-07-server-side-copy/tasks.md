## 1. Core types

- [x] 1.1 Add `CopyMonitorResponse` struct to `crates/cloudmount-core/src/types.rs` with fields: `status: String`, `percentage_complete: Option<f64>` (serde rename `percentageComplete`), `resource_id: Option<String>` (serde rename `resourceId`), `error: Option<GraphErrorBody>`. Derive `Debug, Clone, Serialize, Deserialize`.

## 2. Graph client copy methods

- [x] 2.1 Add `CopyStatus` enum to `crates/cloudmount-graph/src/client.rs` with variants: `InProgress { percentage: f64 }`, `Completed { resource_id: String }`, `Failed { message: String }`. Derive `Debug, Clone`.
- [x] 2.2 Implement `GraphClient::copy_item(&self, drive_id: &str, item_id: &str, dest_drive_id: &str, dest_parent_id: &str, dest_name: &str) -> Result<String>` that POSTs to `{base_url}/drives/{drive_id}/items/{item_id}/copy` with `{ "parentReference": { "driveId": dest_drive_id, "id": dest_parent_id }, "name": dest_name }`, uses `with_retry`, expects HTTP 202, extracts and returns the `Location` header value as the monitor URL.
- [x] 2.3 Implement `GraphClient::poll_copy_status(&self, monitor_url: &str) -> Result<CopyStatus>` that GETs the monitor URL without an `Authorization` header, deserializes the response as `CopyMonitorResponse`, and maps it to the `CopyStatus` enum: `"completed"` with `resource_id` -> `Completed`, `"failed"` -> `Failed` (using error message or "unknown error"), otherwise -> `InProgress`.

## 3. Copy polling constants

- [x] 3.1 Add constants to `crates/cloudmount-vfs/src/core_ops.rs`: `COPY_POLL_INITIAL_MS: u64 = 500`, `COPY_POLL_MAX_MS: u64 = 5000`, `COPY_POLL_BACKOFF: u64 = 2`, `COPY_MAX_POLL_DURATION_SECS: u64 = 300`, `COPY_POLL_MAX_RETRIES: u32 = 3`.

## 4. CoreOps copy_file_range

- [x] 4.1 Implement `CoreOps::copy_file_range(&self, ino_in: u64, fh_in: u64, offset_in: u64, ino_out: u64, fh_out: u64, offset_out: u64, len: u64) -> VfsResult<u32>` with server-side copy eligibility check: source item_id not `local:`, `offset_in == 0`, `len >= source_item.size`. If eligible, proceed to server-side copy path (task 4.2). If not eligible, proceed to fallback path (task 4.3).
- [x] 4.2 Implement server-side copy path within `copy_file_range`: resolve source drive_id and item_id, resolve destination parent_id and name from the destination item metadata, call `graph.copy_item()`, run polling loop with exponential backoff (starting at `COPY_POLL_INITIAL_MS`, capped at `COPY_POLL_MAX_MS`, total duration capped at `COPY_MAX_POLL_DURATION_SECS`), retry failed polls up to `COPY_POLL_MAX_RETRIES` times, on `Completed` fetch new item via `graph.get_item()`, call `inodes.reassign()` for the destination inode, update memory cache, remove writeback entry for old `local:` ID, update the destination open file handle metadata (size, non-dirty), return `source_item.size as u32`.
- [x] 4.3 Implement buffer-level fallback path within `copy_file_range`: read `len` bytes from source handle's buffer at `offset_in` via `open_files.get(fh_in)`, write into destination handle's buffer at `offset_out` via `open_files.get_mut(fh_out)` (resizing if needed), mark destination dirty, update destination item size in memory cache, return bytes copied as `u32`.

## 5. FUSE backend wiring

- [x] 5.1 Add `CopyFileRangeFlags` to the fuser import list in `crates/cloudmount-vfs/src/fuse_fs.rs`.
- [x] 5.2 Implement `copy_file_range` in the `Filesystem` trait impl for `CloudMountFs`: delegate to `self.ops.copy_file_range(ino_in.0, fh_in.0, offset_in, ino_out.0, fh_out.0, offset_out, len)`, on success call `reply.written(n)`, on error call `reply.error(Self::vfs_err_to_errno(e))`.

## 6. Tests

- [x] 6.1 Add Graph client tests in `crates/cloudmount-graph/tests/`: test `copy_item` returns monitor URL from wiremock 202 response with `Location` header; test `copy_item` retries on 429/500; test `copy_item` fails on 404.
- [x] 6.2 Add Graph client tests for `poll_copy_status`: test returns `Completed` with `resourceId` on `"completed"` status; test returns `InProgress` on `"inProgress"` status; test returns `Failed` on `"failed"` status; test no `Authorization` header is sent.
- [x] 6.3 Add CoreOps tests: test server-side copy eligibility detection (eligible when source is remote, offset_in == 0, len >= size; not eligible when source is `local:`, or offset_in > 0, or len < size).
- [x] 6.4 Add CoreOps tests: test buffer-level fallback correctly copies bytes between open file handles at specified offsets.

## 7. Build verification

- [x] 7.1 Run `cargo build --all-targets` and verify zero errors.
- [x] 7.2 Run `cargo test --all-targets` and verify all tests pass.
- [x] 7.3 Run `cargo clippy --all-targets --all-features` and verify zero warnings.
