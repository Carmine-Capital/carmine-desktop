## Why

Opening a remote file that is not in the disk cache blocks the FUSE/CfApi thread until the entire file is downloaded. For a 500 MB video this means the application (or file manager) hangs for minutes with no progress feedback. The open-file-table change (commit 007e409) explicitly deferred this as "future streaming work (change #3)." The `download_range()` method already exists in the Graph client but is never called.

## What Changes

- Replace the eager full-download in `CoreOps::open_file()` with a hybrid strategy: small files (< 4 MB) still download fully on open; large files return a file handle immediately and download in the background.
- Introduce a `DownloadState` abstraction inside `OpenFile` that tracks download progress and lets `read_handle()` either return available bytes instantly or block until the needed range is downloaded.
- For random-access reads into regions not yet downloaded, fall back to on-demand range requests via `GraphClient::download_range()`.
- Cancel in-flight background downloads when `release_file()` is called before the download completes.
- Populate the disk cache once the full download finishes so subsequent opens are instant.
- Propagate download errors (network failures, auth expiry) to blocked readers as `VfsError::IoError`.

## Capabilities

### New Capabilities

_(none — this is an internal performance optimization within existing capabilities)_

### Modified Capabilities

- `virtual-filesystem`: The open-file-table behavior changes. `open()` for large uncached files returns immediately instead of blocking. `read()` may block waiting for download progress instead of always returning instantly from a fully-loaded buffer. `release()` cancels in-progress downloads.
- `graph-client`: The "Download large file" scenario is updated — large file downloads now use streaming with progress tracking rather than a single monolithic `download_content()` call. The existing `download_range()` method is now actively used for random-access patterns.

## Impact

- **Code**: `crates/carminedesktop-vfs/src/core_ops.rs` (major — `OpenFile`, `OpenFileTable`, `open_file`, `read_handle`, `release_file`), `crates/carminedesktop-vfs/src/fuse_fs.rs` (minor — no API change, just different blocking behavior), `crates/carminedesktop-vfs/src/cfapi.rs` (minor — `fetch_data` benefits from range-aware reads), `crates/carminedesktop-graph/src/client.rs` (minor — add streaming download helper alongside existing `download_content`/`download_range`).
- **Tests**: New unit tests for `DownloadState` progress tracking, integration tests for streaming open/read/release lifecycle, tests for cancellation and error propagation.
- **Dependencies**: None added. Uses existing `tokio::sync` primitives (`watch`, `Notify`), `bytes`, and `dashmap`.
- **Backwards compatibility**: External FUSE/CfApi behavior is improved (faster open), not broken. Write path is unchanged.
