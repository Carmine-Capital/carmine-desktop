## MODIFIED Requirements

### Requirement: File attribute retrieval (getattr)
The system SHALL return file attributes (size, timestamps, permissions) when requested by the operating system. When the requested inode has at least one open file handle in the OpenFileTable, the system SHALL return the size from the handle's content buffer (for `DownloadState::Complete`) or the streaming buffer's `total_size` (for `DownloadState::Streaming`) instead of the memory cache size. When no open handle exists, the system SHALL return attributes from the memory cache as before. When an open handle exists, the FUSE reply TTL for the attributes SHALL be 0 seconds to ensure the kernel re-queries on every `stat()` call.

#### Scenario: Get attributes of a file
- **WHEN** the OS requests attributes for a file inode
- **THEN** the system returns: file size in bytes, last modified time (from Graph API `lastModifiedDateTime`), creation time (from `createdDateTime`), file type (regular file), and permissions (0644 for writable files, 0444 for read-only)

#### Scenario: Get attributes of a directory
- **WHEN** the OS requests attributes for a directory inode
- **THEN** the system returns: size 0, timestamps from Graph API, file type (directory), and permissions (0755)

#### Scenario: Get attributes of a file with open handle returns handle-consistent size
- **WHEN** the OS requests attributes for a file inode that has an open handle with content of N bytes, and the memory cache reports a different size M bytes
- **THEN** the system returns size=N (from the handle's content buffer) with a TTL of 0 seconds

#### Scenario: Get attributes of a file with streaming handle returns total size
- **WHEN** the OS requests attributes for a file inode that has an open handle in `DownloadState::Streaming` with `total_size` T
- **THEN** the system returns size=T (the expected final size) with a TTL of 0 seconds

### Requirement: Open file table with per-handle content buffering
The system SHALL maintain an open file table that maps file handles to in-memory content buffers. Each `open()` call SHALL load the file content once and return a unique file handle. Before serving content from the disk cache, the system SHALL validate freshness by checking the dirty-inode set, comparing the disk cache eTag against the metadata eTag, and comparing the content length against the metadata size. If any check fails, the disk cache entry SHALL be discarded and content SHALL be re-downloaded from the Graph API. For files smaller than 4 MB or files with valid disk cache content, content SHALL be loaded eagerly before the file handle is returned. For uncached files of 4 MB or larger, the system SHALL return the file handle immediately and download content in the background via a streaming download task. Subsequent `read()` and `write()` calls SHALL operate on the buffer associated with the file handle, not the inode. The open file table SHALL be shared between FUSE and CfApi backends via `CoreOps`. Each `OpenFile` entry SHALL include a `stale` flag (default `false`) that the delta sync observer can set to `true` when remote content changes are detected. The OpenFileTable SHALL expose a method to query the content size for a given inode (`get_content_size_by_ino`), and a method to mark all handles for a given inode as stale (`mark_stale_by_ino`).

#### Scenario: Open loads content once (small or cached file)
- **WHEN** an application opens a file that is smaller than 4 MB or is already present in the disk cache with valid content
- **THEN** the system loads the file content from writeback buffer, validated disk cache, or Graph API (in that order), stores it in an `OpenFile` entry with `DownloadState::Complete`, allocates a unique file handle, sets `stale` to `false`, and returns it to the caller

#### Scenario: Open validates disk cache before serving
- **WHEN** an application opens a file and the disk cache contains content for that file
- **THEN** the system checks: (1) the inode is NOT in the dirty-inode set, (2) the disk cache eTag matches the metadata eTag (if both are available), and (3) the content length matches the metadata size
- **AND** if all checks pass, the system serves the disk-cached content
- **AND** if any check fails, the system discards the disk cache entry, downloads fresh content from the Graph API, and stores the new content in the disk cache with the current eTag

#### Scenario: Open skips disk cache for dirty inode
- **WHEN** an application opens a file whose inode is in the dirty-inode set
- **THEN** the system skips the disk cache entirely, downloads fresh content from the Graph API, stores it in the disk cache with the current eTag, and removes the inode from the dirty-inode set

#### Scenario: Open returns immediately for large uncached file
- **WHEN** an application opens a file that is 4 MB or larger and is not present in the disk cache or writeback buffer (or disk cache content is stale)
- **THEN** the system pre-allocates a streaming buffer to the file's known size, spawns a background download task, stores the `OpenFile` entry with `DownloadState::Streaming`, allocates a unique file handle, sets `stale` to `false`, and returns it to the caller without waiting for any bytes to download

#### Scenario: Open for writing
- **WHEN** an application opens a file for writing
- **THEN** the system loads existing content (if any) into the `OpenFile` buffer using the same freshness-validated path, marks the access mode as writable, and returns a unique file handle

#### Scenario: Multiple handles to same inode
- **WHEN** two applications open the same file simultaneously
- **THEN** each receives an independent file handle with its own content buffer, independent download state, and independent `stale` flag

#### Scenario: Release frees buffer
- **WHEN** the last file handle for a file is released
- **THEN** the system drops the content buffer from memory

#### Scenario: Query content size by inode
- **WHEN** `get_content_size_by_ino` is called for an inode with an open handle
- **THEN** the system returns the content length (for `Complete`) or `total_size` (for `Streaming`) from the first matching handle
- **AND** if no handle exists for the inode, the system returns `None`

#### Scenario: Mark handles stale by inode
- **WHEN** `mark_stale_by_ino` is called for an inode
- **THEN** the system iterates all open handles and sets `stale = true` on every handle whose `ino` matches the given inode
