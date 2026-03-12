## MODIFIED Requirements

### Requirement: File write operations
The system SHALL buffer writes in the `OpenFile` content buffer associated with the file handle and flush to the writeback buffer on `flush`/`release`. Writing to a file with an in-progress streaming download SHALL block until the download completes. On flush, the system SHALL persist content to the writeback cache and delegate upload to the `SyncProcessor` instead of uploading inline.

Each `OpenFile` entry SHALL maintain a `logical_size: Option<usize>` field. When `truncate()` resizes the buffer to a smaller size, the system SHALL set `logical_size = Some(new_size)`. When no explicit truncation has occurred, `logical_size` SHALL be `None`, and the buffer length SHALL be used as the file size. On `flush_handle()`, the system SHALL truncate the buffer to `logical_size` (if set) before writing to the writeback cache.

The `flush_handle` method SHALL accept a `wait_for_completion: bool` parameter. When `true` and a `SyncHandle` is available, the system SHALL send a `SyncRequest::FlushSync` to the sync processor and block until the upload completes or a 60-second timeout expires. When `false`, the system SHALL use the existing fire-and-forget `SyncRequest::Flush` path.

#### Scenario: Write to a file
- **WHEN** a write operation is issued with a valid file handle whose content is fully available
- **THEN** the system mutates the handle's `OpenFile` content buffer in-place at the specified offset, marks the handle as dirty, updates `logical_size` to `Some(max(logical_size.unwrap_or(0), offset + data.len()))`, updates the in-memory metadata size to the logical_size value, and returns success immediately without touching the writeback buffer

#### Scenario: Write after truncate preserves logical size
- **WHEN** a file is truncated to N bytes (via `truncate()` or `overwrite()`) and then a write of M bytes is issued at offset 0 where M < original buffer length
- **THEN** `logical_size` is `Some(max(N, 0 + M))` = `Some(M)`, the buffer may be larger than M bytes but the system treats M as the authoritative file size
- **AND** on flush, the buffer is truncated to M bytes before writing to writeback, discarding any trailing stale bytes

#### Scenario: Write without prior truncate preserves POSIX semantics
- **WHEN** a file with buffer length L has a write of M bytes at offset 0 where M < L and no prior `truncate()` was called
- **THEN** `logical_size` remains `None`, the buffer retains its original length L, and the full L-byte buffer is flushed
- **AND** this preserves POSIX semantics where writing at offset 0 does not shrink the file

#### Scenario: Write to a file with in-progress download
- **WHEN** a write operation is issued with a valid file handle whose content is still being downloaded
- **THEN** the system blocks until the background download completes, transitions the download state to complete, then performs the write as normal

#### Scenario: Flush on file close (fire-and-forget)
- **WHEN** a file with pending writes is closed (release/flush) with `wait_for_completion: false` and a `SyncHandle` is available
- **THEN** the system truncates the buffer to `logical_size` (if set), pushes the content to the writeback buffer, persists it to disk for crash safety, sends a `SyncRequest::Flush { ino }` to the sync processor, and returns success immediately without waiting for the upload to complete

#### Scenario: Flush on file close (synchronous)
- **WHEN** a file with pending writes is flushed with `wait_for_completion: true` and a `SyncHandle` is available
- **THEN** the system truncates the buffer to `logical_size` (if set), pushes the content to the writeback buffer, persists it to disk, sends a `SyncRequest::FlushSync { ino, done }` to the sync processor, and blocks until the processor signals completion via the oneshot channel
- **AND** if the upload succeeds, the method returns `Ok(())`
- **AND** if the upload fails, the method returns `Err(VfsError::IoError)` with the failure reason
- **AND** if the 60-second timeout expires, the method returns `Err(VfsError::TimedOut)` and the upload continues in the background

#### Scenario: Flush on file close without sync processor
- **WHEN** a file with pending writes is closed (release/flush) and no `SyncHandle` is available (tests or processor disabled)
- **THEN** the system truncates the buffer to `logical_size` (if set), pushes the content to the writeback buffer, then uploads the complete modified file to the Graph API synchronously using the appropriate upload method (small or chunked), and updates the local metadata with the new eTag

#### Scenario: Write conflict detected
- **WHEN** uploading a modified file and the remote eTag differs from the local eTag (another user modified the file)
- **THEN** the system saves the local version as `<filename>.conflict.<timestamp>` in the same directory, downloads the remote version as the primary file, and emits a notification about the conflict
