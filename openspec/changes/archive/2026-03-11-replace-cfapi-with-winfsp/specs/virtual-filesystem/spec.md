## MODIFIED Requirements

### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, the system SHALL mount the drive using WinFsp via `WinFspMountHandle::mount()`. The mount SHALL accept a directory path (e.g., `C:\Users\<user>\Cloud\OneDrive`) or a drive letter (e.g., `Z:`) as the mount point. No sync root registration, display name, account name, or icon configuration is required — WinFsp mounts appear as regular filesystem volumes.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows with a configured mount point
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates a WinFsp filesystem host, mounts it at the configured directory path or drive letter, starts the host, and the mount becomes accessible to all applications via standard Windows file operations

#### Scenario: Multiple concurrent Windows mounts
- **WHEN** two or more drives are mounted simultaneously on Windows
- **THEN** each mount SHALL have its own independent WinFsp filesystem host, and I/O operations SHALL be dispatched to the correct filesystem context for each mount path

#### Scenario: Root resolution failure
- **WHEN** the drive root item cannot be fetched from the Graph API at mount time (network error, invalid drive_id, auth error)
- **THEN** the mount fails and returns an error; the mount point directory is not registered with FUSE/WinFsp, and the error is logged and surfaced to the caller

### Requirement: Open file table with per-handle content buffering
The system SHALL maintain an open file table that maps file handles to in-memory content buffers. Each `open()` call SHALL load the file content once and return a unique file handle. Before serving content from the disk cache, the system SHALL validate freshness by checking the dirty-inode set, comparing the disk cache eTag against the metadata eTag, and comparing the content length against the metadata size. If any check fails, the disk cache entry SHALL be discarded and content SHALL be re-downloaded from the Graph API. For files smaller than 4 MB or files with valid disk cache content, content SHALL be loaded eagerly before the file handle is returned. For uncached files of 4 MB or larger, the system SHALL return the file handle immediately and download content in the background via a streaming download task. Subsequent `read()` and `write()` calls SHALL operate on the buffer associated with the file handle, not the inode. The open file table SHALL be shared between FUSE and WinFsp backends via `CoreOps`. Each `OpenFile` entry SHALL include a `stale` flag (default `false`) that the delta sync observer can set to `true` when remote content changes are detected. The OpenFileTable SHALL expose a method to query the content size for a given inode (`get_content_size_by_ino`), and a method to mark all handles for a given inode as stale (`mark_stale_by_ino`).

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

### Requirement: Delete operations
The system SHALL support deleting files and folders from mounted drives. After deleting a child, the system SHALL surgically remove the child from the parent's in-memory children cache rather than invalidating the entire parent entry.

#### Scenario: Delete a file
- **WHEN** an application deletes a file (e.g., `rm`, Delete key in Explorer)
- **THEN** the system deletes the item via the Graph API, removes it from all cache tiers, and returns success

#### Scenario: Delete a non-empty folder
- **WHEN** an application attempts to delete a non-empty directory via `rmdir`
- **THEN** the system returns an ENOTEMPTY error (standard POSIX behavior) or `STATUS_DIRECTORY_NOT_EMPTY` (Windows)

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

### Requirement: Pending writes flushed on unmount via shared implementation
On unmount, both the FUSE and WinFsp backends SHALL flush any pending write-back
uploads for the unmounting drive using a single shared implementation. The flush
logic SHALL NOT be duplicated per platform.

The flush procedure SHALL:
- List all pending write-back entries for the drive being unmounted.
- Upload each pending entry to the Graph API.
- Remove each entry from the write-back buffer upon successful upload.
- Enforce a maximum flush duration of 30 seconds; if exceeded, log a warning and
  proceed with unmount (data remains in the write-back buffer for crash recovery).

#### Scenario: Pending writes present on unmount
- **WHEN** a mount is stopped with one or more entries in the write-back buffer for that drive
- **THEN** the system SHALL attempt to upload all pending entries before completing the unmount
- **THEN** successfully uploaded entries SHALL be removed from the write-back buffer
- **THEN** the unmount SHALL complete within 30 seconds regardless of upload outcome

#### Scenario: No pending writes on unmount
- **WHEN** a mount is stopped with no entries in the write-back buffer for that drive
- **THEN** the system SHALL skip the flush step and unmount immediately

#### Scenario: Flush timeout exceeded
- **WHEN** uploading pending writes takes longer than 30 seconds
- **THEN** the system SHALL log a warning indicating how many writes remain pending
- **THEN** the unmount SHALL proceed (remaining writes are preserved in the write-back buffer for crash recovery on next launch)

### Requirement: Platform without copy_file_range support
The `copy_file_range` FUSE operation is Linux-specific. On Windows (WinFsp) and macOS, file copies proceed via the standard read+write path with no behavior change.

#### Scenario: File copy on Windows
- **WHEN** a file copy is performed on Windows
- **THEN** the copy proceeds via WinFsp's standard read+write callbacks without any server-side copy optimization

#### Scenario: File copy on macOS
- **WHEN** a file copy is performed on macOS (which lacks FUSE `copy_file_range`)
- **THEN** the copy proceeds via the existing read+write path with no behavior change

## REMOVED Requirements

### Requirement: CfApi closed callback skips unmodified files
**Reason**: CfApi `closed()` callback is removed entirely. WinFsp handles file close via `cleanup`/`close` callbacks which delegate to CoreOps. CoreOps tracks dirty state per file handle — unmodified files are never flushed.
**Migration**: Dirty-handle tracking in CoreOps already handles this. The `cleanup` callback only calls `flush_handle` for handles that were written to.

### Requirement: TOCTOU-safe placeholder population on Windows
**Reason**: WinFsp has no placeholders. Directory contents are served dynamically via `read_directory` callbacks. There is no separate placeholder creation step that can race with other processes.
**Migration**: No migration needed. WinFsp's `read_directory` serves directory contents on demand from CoreOps.

### Requirement: Resilient CfApi callback error handling
**Reason**: All CfApi callbacks (`fetch_data`, `fetch_placeholders`, `delete`, `rename`, `closed`, `validate_data`, `state_changed`) are removed. WinFsp callbacks delegate to CoreOps and map errors to NTSTATUS codes. Error handling is defined in the `winfsp-filesystem` spec's error mapping requirement.
**Migration**: WinFsp error handling is specified in `winfsp-filesystem` spec requirement "WinFsp error mapping". Each callback returns appropriate NTSTATUS on error.

### Requirement: CfApi fetch_data immediate failure signaling
**Reason**: WinFsp has no `fetch_data` callback or hydration model. File reads go through the `read` callback which delegates to CoreOps and returns NTSTATUS on error. There is no 60-second Windows timeout to work around.
**Migration**: WinFsp `read` callback returns errors immediately via NTSTATUS. No special failure signaling needed.

### Requirement: CfApi closed callback surfaces upload failures
**Reason**: The CfApi `closed()` callback is removed. Upload failure notification is handled by the WinFsp `cleanup` callback, which emits `VfsEvent::UploadFailed` on flush failure (specified in `winfsp-filesystem` spec).
**Migration**: `VfsEvent::UploadFailed` is emitted from WinFsp's `cleanup` callback on flush error, providing the same user notification.

### Requirement: CfApi writeback failure notification
**Reason**: Writeback failure notification was specific to CfApi's `closed` callback flow. With WinFsp, the `cleanup` callback handles flush and emits `VfsEvent::UploadFailed` on failure. The app layer notification pathway (`VfsEvent` -> desktop notification) remains unchanged.
**Migration**: Same `VfsEvent::UploadFailed` mechanism, triggered from WinFsp `cleanup` instead of CfApi `closed`.

### Requirement: CfApi state_changed invalidates parent directory cache
**Reason**: WinFsp has no `state_changed` callback and no placeholder state machine. Cache invalidation happens through delta sync (which updates the cache directly) and through normal filesystem operations (which invalidate parent caches surgically via CoreOps).
**Migration**: Delta sync updates caches. CoreOps invalidates parent caches after mutations. No platform-specific cache invalidation needed.

### Requirement: Lossless path handling on Windows
**Reason**: The CfApi-specific `relative_path` helper and its `OsString` path component handling are removed along with `cfapi.rs`. WinFsp delivers paths as `U16CStr` which the WinFsp backend converts to `OsString` for CoreOps resolution. The lossless conversion concern is addressed in the `winfsp-filesystem` spec's path resolution requirement.
**Migration**: WinFsp path handling is specified in `winfsp-filesystem` spec requirement "WinFsp path-to-inode resolution". The `U16CStr` -> `OsString` conversion preserves all valid Windows filenames.
