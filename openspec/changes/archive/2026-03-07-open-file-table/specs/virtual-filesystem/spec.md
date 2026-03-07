## ADDED Requirements

### Requirement: Open file table with per-handle content buffering
The system SHALL maintain an open file table that maps file handles to in-memory content buffers. Each `open()` call SHALL load the file content once and return a unique file handle. Subsequent `read()` and `write()` calls SHALL operate on the buffer associated with the file handle, not the inode. The open file table SHALL be shared between FUSE and CfApi backends via `CoreOps`.

#### Scenario: Open loads content once
- **WHEN** an application opens a file for reading
- **THEN** the system loads the file content from writeback buffer, disk cache, or Graph API (in that order), stores it in an `OpenFile` entry, allocates a unique file handle, and returns it to the caller

#### Scenario: Open for writing
- **WHEN** an application opens a file for writing
- **THEN** the system loads existing content (if any) into the `OpenFile` buffer, marks the access mode as writable, and returns a unique file handle

#### Scenario: Multiple handles to same inode
- **WHEN** two applications open the same file simultaneously
- **THEN** each receives an independent file handle with its own content buffer

#### Scenario: Release frees buffer
- **WHEN** the last file handle for a file is released
- **THEN** the system drops the content buffer from memory

### Requirement: Handle-based release operation
The system SHALL implement `release()` to clean up open file state. On release of a dirty handle, the system SHALL flush content to the writeback buffer before dropping the buffer.

#### Scenario: Release clean handle
- **WHEN** a file handle that was only read (not written) is released
- **THEN** the system drops the content buffer without any upload or writeback activity

#### Scenario: Release dirty handle
- **WHEN** a file handle with pending writes is released without a prior flush
- **THEN** the system pushes the buffer content to the writeback buffer for later upload, then drops the buffer

## MODIFIED Requirements

### Requirement: File read operations
The system SHALL serve file read requests from the content buffer associated with the file handle.

#### Scenario: Read cached file
- **WHEN** a read operation is issued with a valid file handle
- **THEN** the system slices the requested byte range from the handle's `OpenFile` content buffer and returns it without any cache lookup or API call

#### Scenario: Read uncached file
- **WHEN** a read operation is issued for a file not in the disk cache
- **THEN** on `open()`, the system downloads the file content from the Graph API, writes it to the disk cache, stores it in the `OpenFile` buffer, and subsequent `read()` calls return slices from that buffer

#### Scenario: Sequential read of large file
- **WHEN** a file larger than 64 MB is being read sequentially
- **THEN** the system uses read-ahead prefetching, downloading the next 16 MB chunk while the current chunk is being read

### Requirement: File write operations
The system SHALL buffer writes in the `OpenFile` content buffer associated with the file handle and flush to the writeback buffer on `flush`/`release`.

#### Scenario: Write to a file
- **WHEN** a write operation is issued with a valid file handle
- **THEN** the system mutates the handle's `OpenFile` content buffer in-place at the specified offset, marks the handle as dirty, updates the in-memory metadata size, and returns success immediately without touching the writeback buffer

#### Scenario: Flush on file close
- **WHEN** a file with pending writes is closed (release/flush)
- **THEN** the system pushes the `OpenFile` buffer content to the writeback buffer, then uploads the complete modified file to the Graph API using the appropriate upload method (small or chunked), and updates the local metadata with the new eTag

#### Scenario: Write conflict detected
- **WHEN** uploading a modified file and the remote eTag differs from the local eTag (another user modified the file)
- **THEN** the system saves the local version as `<filename>.conflict.<timestamp>` in the same directory, downloads the remote version as the primary file, and emits a notification about the conflict

### Requirement: File and folder creation
The system SHALL support creating new files and folders in mounted drives. `create()` SHALL return an open file handle for the new file.

#### Scenario: Create new empty file
- **WHEN** an application creates a new file (e.g., `touch`, Save As)
- **THEN** the system creates a placeholder in the local cache, assigns a temporary inode, creates an `OpenFile` entry with an empty buffer, returns the file handle, and uploads the file to the Graph API on flush/close

#### Scenario: Create new folder
- **WHEN** an application creates a new directory (e.g., `mkdir`)
- **THEN** the system creates the folder via the Graph API immediately and returns the new inode
