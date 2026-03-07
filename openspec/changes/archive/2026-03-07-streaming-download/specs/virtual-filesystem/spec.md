## MODIFIED Requirements

### Requirement: Open file table with per-handle content buffering
The system SHALL maintain an open file table that maps file handles to in-memory content buffers. Each `open()` call SHALL load the file content once and return a unique file handle. For files smaller than 4 MB or files already present in the disk cache, content SHALL be loaded eagerly before the file handle is returned. For uncached files of 4 MB or larger, the system SHALL return the file handle immediately and download content in the background via a streaming download task. Subsequent `read()` and `write()` calls SHALL operate on the buffer associated with the file handle, not the inode. The open file table SHALL be shared between FUSE and CfApi backends via `CoreOps`.

#### Scenario: Open loads content once (small or cached file)
- **WHEN** an application opens a file that is smaller than 4 MB or is already present in the disk cache
- **THEN** the system loads the file content from writeback buffer, disk cache, or Graph API (in that order), stores it in an `OpenFile` entry with `DownloadState::Complete`, allocates a unique file handle, and returns it to the caller

#### Scenario: Open returns immediately for large uncached file
- **WHEN** an application opens a file that is 4 MB or larger and is not present in the disk cache or writeback buffer
- **THEN** the system pre-allocates a streaming buffer to the file's known size, spawns a background download task, stores the `OpenFile` entry with `DownloadState::Streaming`, allocates a unique file handle, and returns it to the caller without waiting for any bytes to download

#### Scenario: Open for writing
- **WHEN** an application opens a file for writing
- **THEN** the system loads existing content (if any) into the `OpenFile` buffer, marks the access mode as writable, and returns a unique file handle

#### Scenario: Multiple handles to same inode
- **WHEN** two applications open the same file simultaneously
- **THEN** each receives an independent file handle with its own content buffer and independent download state

#### Scenario: Release frees buffer
- **WHEN** the last file handle for a file is released
- **THEN** the system drops the content buffer from memory

### Requirement: Handle-based release operation
The system SHALL implement `release()` to clean up open file state. On release of a dirty handle, the system SHALL flush content to the writeback buffer before dropping the buffer. On release of a handle with an in-progress streaming download, the system SHALL cancel the background download task.

#### Scenario: Release clean handle
- **WHEN** a file handle that was only read (not written) is released
- **THEN** the system drops the content buffer without any upload or writeback activity

#### Scenario: Release dirty handle
- **WHEN** a file handle with pending writes is released without a prior flush
- **THEN** the system pushes the buffer content to the writeback buffer for later upload, then drops the buffer

#### Scenario: Release handle with in-progress download
- **WHEN** a file handle is released while a background streaming download is still in progress
- **THEN** the system cancels the background download task via its abort handle, drops the incomplete buffer, and does NOT write incomplete content to the disk cache

### Requirement: File read operations
The system SHALL serve file read requests from the content buffer associated with the file handle. For handles with in-progress streaming downloads, the system SHALL block until the requested byte range is available or return an error if the download fails.

#### Scenario: Read from complete buffer
- **WHEN** a read operation is issued with a valid file handle whose download state is complete
- **THEN** the system slices the requested byte range from the handle's content buffer and returns it without any cache lookup or API call

#### Scenario: Sequential read from streaming buffer
- **WHEN** a read operation is issued for a byte range that is not yet downloaded, and the requested offset is within 2 MB of the current download frontier
- **THEN** the system blocks until the background download task has downloaded enough bytes to satisfy the request, then returns the requested byte range

#### Scenario: Random-access read from streaming buffer
- **WHEN** a read operation is issued for a byte range that is not yet downloaded, and the requested offset is more than 2 MB ahead of the current download frontier
- **THEN** the system issues an on-demand range request via `download_range()` for the requested region and returns those bytes, while the background download continues independently

#### Scenario: Read after download failure
- **WHEN** a read operation is issued on a handle whose background download has failed
- **THEN** the system returns an I/O error to the caller

#### Scenario: Read uncached file
- **WHEN** a read operation is issued for a file not in the disk cache
- **THEN** on `open()`, the system initiates a download (eager for small files, streaming for large files), and subsequent `read()` calls return data from the buffer as it becomes available

### Requirement: File write operations
The system SHALL buffer writes in the `OpenFile` content buffer associated with the file handle and flush to the writeback buffer on `flush`/`release`. Writing to a file with an in-progress streaming download SHALL block until the download completes.

#### Scenario: Write to a file
- **WHEN** a write operation is issued with a valid file handle whose content is fully available
- **THEN** the system mutates the handle's `OpenFile` content buffer in-place at the specified offset, marks the handle as dirty, updates the in-memory metadata size, and returns success immediately without touching the writeback buffer

#### Scenario: Write to a file with in-progress download
- **WHEN** a write operation is issued with a valid file handle whose content is still being downloaded
- **THEN** the system blocks until the background download completes, transitions the download state to complete, then performs the write as normal

#### Scenario: Flush on file close
- **WHEN** a file with pending writes is closed (release/flush)
- **THEN** the system pushes the `OpenFile` buffer content to the writeback buffer, then uploads the complete modified file to the Graph API using the appropriate upload method (small or chunked), and updates the local metadata with the new eTag

#### Scenario: Write conflict detected
- **WHEN** uploading a modified file and the remote eTag differs from the local eTag (another user modified the file)
- **THEN** the system saves the local version as `<filename>.conflict.<timestamp>` in the same directory, downloads the remote version as the primary file, and emits a notification about the conflict

### Requirement: Streaming download disk cache population
The system SHALL populate the disk cache with the complete file content when a streaming background download finishes successfully.

#### Scenario: Streaming download completes
- **WHEN** a background streaming download task finishes downloading all bytes of a file
- **THEN** the system writes the complete content to the disk cache so that subsequent `open()` calls for the same file load from disk cache instead of re-downloading

#### Scenario: Streaming download cancelled or failed
- **WHEN** a background streaming download is cancelled (via release) or fails (network error)
- **THEN** the system does NOT write any content to the disk cache; the next `open()` will attempt a fresh download
