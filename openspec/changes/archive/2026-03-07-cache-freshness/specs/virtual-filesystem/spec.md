## MODIFIED Requirements

### Requirement: Open file table with per-handle content buffering
The system SHALL maintain an open file table that maps file handles to in-memory content buffers. Each `open()` call SHALL load the file content once and return a unique file handle. Before serving content from the disk cache, the system SHALL validate freshness by checking the dirty-inode set, comparing the disk cache eTag against the metadata eTag, and comparing the content length against the metadata size. If any check fails, the disk cache entry SHALL be discarded and content SHALL be re-downloaded from the Graph API. For files smaller than 4 MB or files with valid disk cache content, content SHALL be loaded eagerly before the file handle is returned. For uncached files of 4 MB or larger, the system SHALL return the file handle immediately and download content in the background via a streaming download task. Subsequent `read()` and `write()` calls SHALL operate on the buffer associated with the file handle, not the inode. The open file table SHALL be shared between FUSE and CfApi backends via `CoreOps`.

#### Scenario: Open loads content once (small or cached file)
- **WHEN** an application opens a file that is smaller than 4 MB or is already present in the disk cache with valid content
- **THEN** the system loads the file content from writeback buffer, validated disk cache, or Graph API (in that order), stores it in an `OpenFile` entry with `DownloadState::Complete`, allocates a unique file handle, and returns it to the caller

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
- **THEN** the system pre-allocates a streaming buffer to the file's known size, spawns a background download task, stores the `OpenFile` entry with `DownloadState::Streaming`, allocates a unique file handle, and returns it to the caller without waiting for any bytes to download

#### Scenario: Open for writing
- **WHEN** an application opens a file for writing
- **THEN** the system loads existing content (if any) into the `OpenFile` buffer using the same freshness-validated path, marks the access mode as writable, and returns a unique file handle

#### Scenario: Multiple handles to same inode
- **WHEN** two applications open the same file simultaneously
- **THEN** each receives an independent file handle with its own content buffer and independent download state

#### Scenario: Release frees buffer
- **WHEN** the last file handle for a file is released
- **THEN** the system drops the content buffer from memory

### Requirement: FUSE mount options for performance (Linux/macOS)

The FUSE mount must configure kernel-level performance options to maximize I/O throughput. The attribute and entry TTL for files SHALL be shorter than for directories, to balance responsiveness to remote changes with cache efficiency.

#### Scenario: Mount includes max_read option

- **WHEN** CloudMountFs mounts on Linux or macOS
- **THEN** the mount options include `max_read=1048576` (1MB)
- **AND** the kernel sends read requests up to 1MB instead of the default 128KB

#### Scenario: init() enables writeback cache capability

- **WHEN** the FUSE `init()` callback is invoked
- **THEN** the filesystem requests `FUSE_CAP_WRITEBACK_CACHE` via `KernelConfig::add_capabilities()`
- **AND** if the kernel supports it, writes are coalesced before reaching userspace
- **AND** if the kernel does not support it, the mount proceeds without it (graceful degradation)

#### Scenario: init() enables parallel directory operations

- **WHEN** the FUSE `init()` callback is invoked
- **THEN** the filesystem requests `FUSE_CAP_PARALLEL_DIROPS` via `KernelConfig::add_capabilities()`
- **AND** if the kernel supports it, directory operations are not serialized

#### Scenario: NoAtime reduces unnecessary metadata updates

- **WHEN** CloudMountFs mounts
- **THEN** the mount options include `NoAtime`
- **AND** file access times are not updated on read

#### Scenario: File attribute TTL is shorter than directory TTL

- **WHEN** the filesystem replies to a `getattr` or `lookup` for a regular file
- **THEN** the TTL included in the reply SHALL be 5 seconds

#### Scenario: Directory attribute TTL is longer

- **WHEN** the filesystem replies to a `getattr` or `lookup` for a directory
- **THEN** the TTL included in the reply SHALL be 30 seconds
