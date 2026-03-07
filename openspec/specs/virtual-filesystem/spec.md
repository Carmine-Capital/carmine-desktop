### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, each CfApi mount SHALL use a unique sync root ID by including an `account_name` discriminator in the sync root ID construction. The sync root ID format SHALL be `<provider>!<security-id>!<account_name>`. The `account_name` parameter SHALL be required when calling `CfMountHandle::mount()`.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows with an `account_name` identifier
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, registers a Cloud Files API sync root with a unique sync root ID derived from the provider name, user security ID, and account name, populates the directory with placeholder files, and the sync root appears as a first-class entry in File Explorer's navigation pane with cloud sync status icons

#### Scenario: Multiple concurrent Windows mounts
- **WHEN** two or more drives are mounted simultaneously on Windows, each with a distinct `account_name`
- **THEN** each mount SHALL have its own independent sync root registration, and CfApi callbacks SHALL be dispatched to the correct filter for each mount path

#### Scenario: Root resolution failure
- **WHEN** the drive root item cannot be fetched from the Graph API at mount time (network error, invalid drive_id, auth error)
- **THEN** the mount fails and returns an error; the mount point directory is not registered with FUSE/CfApi, and the error is logged and surfaced to the caller

### Requirement: Directory listing (readdir)
The system SHALL return directory contents when the operating system requests a directory listing. On Linux/macOS, the system SHALL implement both `readdir` and `readdirplus` FUSE operations. `readdirplus` SHALL return directory entries together with full file attributes in a single FUSE response, eliminating the need for per-entry `getattr` calls.

#### Scenario: List folder contents
- **WHEN** a user or application reads a mounted directory (e.g., `ls`, File Explorer browse)
- **THEN** the system returns the list of files and subdirectories with their names, sizes, types (file/folder), and modification times, sourced from the metadata cache or fetched from the Graph API on cache miss

#### Scenario: Large directory (> 1000 items)
- **WHEN** a directory contains more than 1000 items
- **THEN** the system returns all items without truncation, paginating through the Graph API as needed, and caches the complete listing

#### Scenario: readdirplus returns entries with attributes
- **WHEN** the kernel issues a `readdirplus` request for a directory
- **THEN** the system returns each child entry together with its full `FileAttr` (size, timestamps, type, permissions) and a TTL, using the same data from `CoreOps::list_children`
- **AND** the kernel caches the returned attributes, avoiding separate `getattr` calls for each entry

#### Scenario: readdirplus offset handling
- **WHEN** a `readdirplus` request includes a non-zero offset
- **THEN** the system skips entries up to that offset and returns entries starting from the offset position
- **AND** if the reply buffer fills before all entries are returned, the system stops and the kernel issues a follow-up request with the next offset

#### Scenario: readdirplus dot entries
- **WHEN** a `readdirplus` request is issued for a directory
- **THEN** the system includes `.` and `..` entries with directory type and the parent directory's attributes before the regular child entries

### Requirement: File attribute retrieval (getattr)
The system SHALL return file attributes (size, timestamps, permissions) when requested by the operating system.

#### Scenario: Get attributes of a file
- **WHEN** the OS requests attributes for a file inode
- **THEN** the system returns: file size in bytes, last modified time (from Graph API `lastModifiedDateTime`), creation time (from `createdDateTime`), file type (regular file), and permissions (0644 for writable files, 0444 for read-only)

#### Scenario: Get attributes of a directory
- **WHEN** the OS requests attributes for a directory inode
- **THEN** the system returns: size 0, timestamps from Graph API, file type (directory), and permissions (0755)

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

### Requirement: File and folder creation
The system SHALL support creating new files and folders in mounted drives. `create()` SHALL return an open file handle for the new file. After creating a child, the system SHALL surgically insert the new child into the parent's in-memory children cache rather than invalidating the entire parent entry.

#### Scenario: Create new empty file
- **WHEN** an application creates a new file (e.g., `touch`, Save As)
- **THEN** the system creates a placeholder in the local cache, assigns a temporary inode, creates an `OpenFile` entry with an empty buffer, returns the file handle, and uploads the file to the Graph API on flush/close

#### Scenario: Create new folder
- **WHEN** an application creates a new directory (e.g., `mkdir`)
- **THEN** the system creates the folder via the Graph API immediately and returns the new inode

#### Scenario: Create updates parent cache surgically
- **WHEN** a new file is created in a directory whose children are cached in memory
- **THEN** the system inserts the new child's name and inode into the parent's children `HashMap`
- **AND** the parent's existing children and metadata remain unchanged
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Mkdir updates parent cache surgically
- **WHEN** a new folder is created in a directory whose children are cached in memory
- **THEN** the system inserts the new folder's name and inode into the parent's children `HashMap`
- **AND** the parent's existing children and metadata remain unchanged

### Requirement: Delete operations
The system SHALL support deleting files and folders from mounted drives. After deleting a child, the system SHALL surgically remove the child from the parent's in-memory children cache rather than invalidating the entire parent entry.

#### Scenario: Delete a file
- **WHEN** an application deletes a file (e.g., `rm`, Delete key in Explorer)
- **THEN** the system deletes the item via the Graph API, removes it from all cache tiers, and returns success

#### Scenario: Delete a non-empty folder
- **WHEN** an application attempts to delete a non-empty directory via `rmdir`
- **THEN** the system returns an ENOTEMPTY error (standard POSIX behavior)

#### Scenario: Unlink updates parent cache surgically
- **WHEN** a file is deleted from a directory whose children are cached in memory
- **THEN** the system removes the deleted child's name from the parent's children `HashMap`
- **AND** the parent's remaining children and metadata remain unchanged
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Rmdir updates parent cache surgically
- **WHEN** an empty folder is deleted from a directory whose children are cached in memory
- **THEN** the system removes the deleted folder's name from the parent's children `HashMap`
- **AND** the parent's remaining children and metadata remain unchanged

### Requirement: Rename and move operations
The system SHALL support renaming and moving files and folders within a mounted drive. After renaming or moving a child, the system SHALL surgically update the affected parent directories' in-memory children caches rather than invalidating them.

#### Scenario: Rename a file in the same directory
- **WHEN** an application renames a file
- **THEN** the system calls the Graph API to rename the item and updates all cache entries

#### Scenario: Move a file to a different directory
- **WHEN** an application moves a file to a different directory within the same mount
- **THEN** the system calls the Graph API to move the item and updates the parent references in all cache tiers

#### Scenario: Rename updates parent cache surgically
- **WHEN** a file or folder is renamed within the same directory
- **THEN** the system removes the old name from the parent's children `HashMap` and inserts the new name with the same inode
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Cross-directory move updates both parents surgically
- **WHEN** a file or folder is moved from one directory to another
- **THEN** the system removes the old name from the source parent's children `HashMap` and inserts the new name into the destination parent's children `HashMap`
- **AND** no Graph API `list_children` call is triggered for either parent directory

### Requirement: O(1) child lookup by name
The system SHALL look up a child item by name under a parent directory in O(1) time using the parent's children `HashMap`, instead of iterating all children.

#### Scenario: find_child with populated cache
- **WHEN** `find_child` is called for a parent whose children are cached in memory
- **THEN** the system looks up the child name directly in the parent's `HashMap<String, u64>` and returns the matching inode and `DriveItem` without iterating other children

#### Scenario: find_child cache miss falls back to SQLite then Graph API
- **WHEN** `find_child` is called for a parent whose children are not in memory cache
- **THEN** the system falls back to SQLite, then Graph API, populating the parent's children `HashMap` on Graph API response
- **AND** the populated `HashMap` is keyed by child name for subsequent O(1) lookups

### Requirement: Graceful unmount
The system SHALL cleanly unmount drives without data loss.

#### Scenario: User-initiated unmount
- **WHEN** the user clicks "Unmount" in the tray app
- **THEN** the system flushes all pending writes, waits for in-flight uploads to complete (with a 30-second timeout), unmounts the FUSE filesystem, and confirms unmount to the user

#### Scenario: Forced unmount on shutdown
- **WHEN** the system receives a shutdown signal (SIGTERM, system reboot)
- **THEN** the system flushes pending writes with a 10-second timeout, forcefully unmounts the FUSE filesystem, and saves any unflushed changes to a pending-uploads queue for retry on next start

### Requirement: FUSE mount options for performance (Linux/macOS)

The FUSE mount must configure kernel-level performance options to maximize I/O throughput.

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

### Requirement: Server-side copy via copy_file_range
The system SHALL implement the FUSE `copy_file_range` operation to optimize file copies within the mount. When both source and destination are remote items and the copy covers the full file, the system SHALL use the Graph API server-side copy instead of transferring data through the client. When server-side copy is not eligible, the system SHALL fall back to an in-memory buffer copy between the open file handles.

#### Scenario: Full-file copy between two remote files
- **WHEN** `copy_file_range` is called with `offset_in == 0`, `len >= source file size`, and both the source item ID and destination parent are remote (not `local:` prefixed)
- **THEN** the system calls the Graph API copy endpoint, polls for completion, retrieves the new item metadata, reassigns the destination inode from its temporary `local:` ID to the real server item ID, updates all caches with the new item metadata, and returns the number of bytes copied (equal to the source file size)

#### Scenario: Partial range copy
- **WHEN** `copy_file_range` is called with `offset_in > 0` or `len < source file size`
- **THEN** the system falls back to reading the requested byte range from the source file handle's in-memory buffer and writing it into the destination file handle's buffer at `offset_out`, marking the destination handle as dirty

#### Scenario: Copy from a local (not yet uploaded) file
- **WHEN** `copy_file_range` is called and the source item ID starts with `local:`
- **THEN** the system falls back to the in-memory buffer copy between file handles

#### Scenario: Buffer-level fallback copies data in-memory
- **WHEN** server-side copy is not eligible and the system falls back to buffer-level copy
- **THEN** the system reads from the source handle's `OpenFile` content buffer and writes into the destination handle's `OpenFile` content buffer without any network I/O, and returns the number of bytes copied

#### Scenario: Server-side copy updates destination inode mapping
- **WHEN** a server-side copy completes successfully
- **THEN** the system calls `InodeTable::reassign()` to update the destination inode from its temporary `local:` ID to the server-assigned item ID, inserts the new `DriveItem` into the memory cache, and removes any writeback buffer entry for the old temporary ID

#### Scenario: Server-side copy failure returns error
- **WHEN** the Graph API copy operation fails (HTTP error, server-side failure, or timeout)
- **THEN** the system returns `EIO` to the FUSE caller and logs the error details

#### Scenario: Destination file handle updated after server-side copy
- **WHEN** a server-side copy completes and the destination file handle is still open
- **THEN** the system updates the open file handle's inode metadata to reflect the copied file's size and marks the handle as non-dirty (the server already has the complete data)

#### Scenario: Platform without copy_file_range support
- **WHEN** a file copy is performed on macOS (which lacks FUSE `copy_file_range`) or on Windows (CfApi)
- **THEN** the copy proceeds via the existing read+write path with no behavior change
