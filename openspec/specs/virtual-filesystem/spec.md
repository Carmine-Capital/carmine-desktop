### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, each CfApi mount SHALL use a unique sync root ID by including an `account_name` discriminator in the sync root ID construction. The sync root ID format SHALL be `<provider>!<security-id>!<account_name>`. The `account_name` parameter SHALL be required when calling `CfMountHandle::mount()`. The `account_name` value MUST NOT contain `!` (exclamation mark) characters, as `!` is the sync root ID component separator. When constructing the account_name from a Microsoft Graph drive ID, the caller SHALL replace all `!` characters with `_` before passing it to the mount function.

On Windows, `CfMountHandle::mount()` SHALL accept a `display_name` parameter separate from `account_name`. The sync root SHALL be registered with `display_name` as the user-visible label shown in File Explorer's navigation pane. The `display_name` SHALL be the user-visible mount name (e.g., the value of `mount_config.name`) without `!`-sanitization. The sync root SHALL be registered unconditionally on every mount call (not only when previously unregistered) so that stale display names from prior launches are corrected.

On Windows, the sync root icon SHALL reference the running application executable at index 0 (e.g., `<path-to-cloudmount.exe>,0`), resolved via `std::env::current_exe()` at mount time. If the executable path cannot be determined, the system SHALL fall back to a shell-provided cloud folder icon (`%SystemRoot%\system32\shell32.dll,43`). The icon MUST NOT reference `imageres.dll,0`.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows with an `account_name` identifier and a `display_name`
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, registers a Cloud Files API sync root unconditionally (overwriting any prior registration) with a unique sync root ID derived from the provider name, user security ID, and account name; the registration uses `display_name` as the File Explorer label and the application executable as the icon source; the sync root appears in File Explorer's navigation pane with the correct mount name and the CloudMount app icon

#### Scenario: Windows sync root display name matches mount name
- **WHEN** two or more drives are mounted on Windows with distinct names (e.g., "Adelya" and "Alpha Nova")
- **THEN** each mount appears in File Explorer's navigation pane with its own user-visible name, not the generic provider name "CloudMount"

#### Scenario: Windows sync root icon shows application icon
- **WHEN** a mount is registered on Windows and `std::env::current_exe()` succeeds
- **THEN** the sync root icon path is set to `<exe_path>,0` so File Explorer displays the CloudMount application icon next to each mount entry

#### Scenario: Windows sync root icon fallback
- **WHEN** a mount is registered on Windows and `std::env::current_exe()` fails
- **THEN** the sync root icon falls back to `%SystemRoot%\system32\shell32.dll,43` and the mount is registered successfully without failing

#### Scenario: Stale sync root registration corrected on remount
- **WHEN** a sync root was previously registered with an incorrect display name (e.g., "CloudMount") and the user relaunches the application
- **THEN** the sync root is re-registered unconditionally with the correct display name and icon, and File Explorer reflects the updated label without requiring a manual unmount/remount

#### Scenario: Mount on Windows with drive ID containing exclamation marks
- **WHEN** the user enables a mount on Windows and the drive ID contains `!` characters (e.g., SharePoint/OneDrive Business `b!...` format)
- **THEN** the system sanitizes the account_name by replacing all `!` with `_` before constructing the sync root ID, producing a valid 3-component ID (`provider!SID!account_name_without_bangs`); the `display_name` is NOT sanitized and retains the original mount name as-is

#### Scenario: Multiple concurrent Windows mounts
- **WHEN** two or more drives are mounted simultaneously on Windows, each with a distinct `account_name`
- **THEN** each mount SHALL have its own independent sync root registration, and CfApi callbacks SHALL be dispatched to the correct filter for each mount path

#### Scenario: Root resolution failure
- **WHEN** the drive root item cannot be fetched from the Graph API at mount time (network error, invalid drive_id, auth error)
- **THEN** the mount fails and returns an error; the mount point directory is not registered with FUSE/CfApi, and the error is logged and surfaced to the caller

### Requirement: Stale FUSE mount detection and cleanup
The system SHALL detect and attempt to clean up stale FUSE mounts before mounting a drive. A stale mount occurs when a previous FUSE daemon exited without proper unmount (crash, kill signal, or `auto_unmount` not supported).

#### Scenario: Stale mount detected via stat
- **WHEN** the system checks a mountpoint path and `stat` returns ENOTCONN (errno 107, "Transport endpoint is not connected") or EIO (errno 5)
- **THEN** the system identifies the path as a stale FUSE mount and attempts cleanup

#### Scenario: Cleanup via fusermount on Linux
- **WHEN** a stale mount is detected on Linux
- **THEN** the system attempts `fusermount3 -u <path>` first; if `fusermount3` is not available or fails, it attempts `fusermount -u <path>`; the result (success or failure) is logged

#### Scenario: Cleanup via umount on macOS
- **WHEN** a stale mount is detected on macOS
- **THEN** the system attempts `umount <path>` to clean up the stale mount

#### Scenario: Cleanup succeeds
- **WHEN** stale mount cleanup succeeds (fusermount/umount returns exit code 0)
- **THEN** the system logs an info message and the mountpoint path becomes a regular directory accessible for `create_dir_all` and subsequent FUSE mount

#### Scenario: Cleanup fails
- **WHEN** stale mount cleanup fails (fusermount/umount returns non-zero or is not found)
- **THEN** the system logs a warning with the error details and an actionable message suggesting manual cleanup (e.g., "run `fusermount -u <path>` manually"), and returns false to indicate the mountpoint is not usable

#### Scenario: Path is not a stale mount
- **WHEN** the system checks a mountpoint path and `stat` succeeds (returns valid metadata) or the path does not exist
- **THEN** the system takes no cleanup action and proceeds with normal mount setup

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
The system SHALL buffer writes in the `OpenFile` content buffer associated with the file handle and flush to the writeback buffer on `flush`/`release`. Writing to a file with an in-progress streaming download SHALL block until the download completes. On Windows, the CfApi `closed` callback SHALL propagate writeback errors instead of silently discarding them. If writeback fails, the callback SHALL log at `error` level, emit a `VfsEvent::WritebackFailed` event, and skip `flush_inode` and `mark_placeholder_synced`.

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

#### Scenario: CfApi closed writeback error
- **WHEN** the CfApi `closed` callback attempts to write file content to the writeback buffer and the write fails (e.g., disk full)
- **THEN** the system logs the error at `error` level with the file path, emits a `VfsEvent::WritebackFailed` event, does NOT call `flush_inode`, and does NOT call `mark_placeholder_synced`
- **AND** the file remains marked as pending so the user is aware the change was not persisted

#### Scenario: CfApi closed streams large files to writeback
- **WHEN** the CfApi `closed` callback processes a file larger than 4 MB
- **THEN** the system reads the file in chunks (64 KiB) and writes each chunk directly to the writeback layer without accumulating the entire file in memory
- **AND** if any chunk write fails, the system logs the error, emits a `VfsEvent::WritebackFailed` event, and aborts the writeback

### Requirement: Conditional upload with If-Match
The system SHALL use the `If-Match` HTTP header when uploading modified files to close the TOCTOU window between conflict detection and upload. When `flush_inode` detects no local conflict (cached eTag matches server eTag), it SHALL pass the server eTag as an `If-Match` header to the upload request. If the server returns 412 Precondition Failed, the system SHALL treat it as a conflict and follow the standard conflict resolution path.

#### Scenario: Upload with matching eTag
- **WHEN** `flush_inode` uploads a modified file and the server eTag has not changed since the conflict check
- **THEN** the upload succeeds with the `If-Match` header and the local metadata is updated with the new eTag

#### Scenario: Upload with stale eTag (412 response)
- **WHEN** `flush_inode` uploads a modified file with an `If-Match` header and the server returns 412 Precondition Failed
- **THEN** the system treats this as a conflict: saves the local version as `<filename>.conflict.<timestamp>`, downloads the server version, and emits a conflict notification

#### Scenario: Upload of newly created file (no eTag)
- **WHEN** `flush_inode` uploads a newly created file that has no server eTag (first upload)
- **THEN** the upload proceeds without an `If-Match` header (no conflict check needed for new files)

### Requirement: Rename conflict copy safety
When a rename operation overwrites an existing destination that has divergent server content, the system SHALL upload a conflict copy of the destination's server content before proceeding with the delete-and-rename. If the conflict copy upload fails, the system SHALL abort the rename and return an error rather than proceeding with the deletion of the original destination.

#### Scenario: Rename conflict copy succeeds
- **WHEN** a rename targets an existing destination with a different server eTag, and the conflict copy uploads successfully
- **THEN** the system deletes the original destination and completes the rename

#### Scenario: Rename conflict copy upload fails
- **WHEN** a rename targets an existing destination with a different server eTag, and the conflict copy upload fails (network error, 5xx)
- **THEN** the system aborts the rename, returns an error to the caller, and the original destination file is preserved unchanged

### Requirement: Memory-efficient content handling
The system SHALL avoid unbounded memory allocations proportional to file size in the following paths: streaming buffer initialization, flush upload, range reads from disk cache, and crash recovery uploads.

#### Scenario: Streaming buffer uses incremental allocation
- **WHEN** an application opens a large file for streaming download
- **THEN** the system allocates memory incrementally as chunks are downloaded, using a chunk-based buffer (e.g., 256 KiB chunks), instead of pre-allocating the entire file size

#### Scenario: flush_inode avoids content clone
- **WHEN** `flush_inode` uploads a modified file
- **THEN** the system moves the content `Vec<u8>` into `Bytes` (zero-copy) instead of cloning it
- **AND** if a conflict is detected, the content is cloned only at that point for the conflict copy upload

#### Scenario: Range read from disk cache
- **WHEN** `read_range_direct` serves a byte range from a file in the disk cache
- **THEN** the system reads only the requested byte range from disk (via seek + read), not the entire file

#### Scenario: Crash recovery streams large pending files
- **WHEN** crash recovery processes a pending write larger than 4 MB
- **THEN** the system uploads it via the chunked upload session (`upload_large`) instead of loading the entire file into memory

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
The system SHALL support deleting files and folders from mounted drives. After deleting a child, the system SHALL surgically remove the child from the parent's in-memory children cache rather than invalidating the entire parent entry. On Windows, the CfApi `delete` callback SHALL delegate to `CoreOps::unlink()` for files and `CoreOps::rmdir()` for folders, using the same shared logic as the FUSE backend. The CfApi callback SHALL resolve the parent inode from the relative path, determine whether the item is a file or folder, and call the appropriate `CoreOps` method. If the `CoreOps` method returns an error, the callback SHALL log at `warn` level and return `Ok(())` without calling `ticket.pass()`, so the OS sees the operation as incomplete and may retry.

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

#### Scenario: CfApi delete delegates to CoreOps
- **WHEN** the CfApi `delete` callback is invoked for a file on Windows
- **THEN** the system resolves the parent inode and child name from the relative path, calls `CoreOps::unlink(parent_ino, name)`, and on success calls `ticket.pass()`
- **AND** if the item is a folder, the system calls `CoreOps::rmdir(parent_ino, name)` instead

#### Scenario: CfApi delete error handling
- **WHEN** the CfApi `delete` callback invokes `CoreOps::unlink()` or `CoreOps::rmdir()` and it returns an error (e.g., network failure, 403 Forbidden, directory not empty)
- **THEN** the system logs the error at `warn` level with the file path and error details, does NOT call `ticket.pass()`, and returns `Ok(())` to the proxy
- **AND** local caches are NOT purged (the `CoreOps` method handles cache cleanup only on success)

### Requirement: Rename and move operations
The system SHALL support renaming and moving files and folders within a mounted drive. After renaming or moving a child, the system SHALL surgically update the affected parent directories' in-memory children caches rather than invalidating them. On Windows, the CfApi `rename` callback SHALL delegate to `CoreOps::rename()`, gaining eTag-based conflict detection on destination overwrite, error propagation, and parent cache invalidation. If the `CoreOps` method returns an error, the callback SHALL log at `warn` level and return `Ok(())` without calling `ticket.pass()`.

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

#### Scenario: CfApi rename delegates to CoreOps
- **WHEN** the CfApi `rename` callback is invoked on Windows
- **THEN** the system resolves source parent inode and child name from the source relative path, resolves destination parent inode and child name from the target relative path, calls `CoreOps::rename(src_parent_ino, src_name, dst_parent_ino, dst_name)`, and on success calls `ticket.pass()`

#### Scenario: CfApi rename conflict detection
- **WHEN** the CfApi `rename` callback invokes `CoreOps::rename()` and the destination file exists with a different eTag on the server
- **THEN** the system creates a `.conflict.{timestamp}` copy of the server version (same as FUSE behavior), completes the rename, and emits a conflict VfsEvent

#### Scenario: CfApi rename error handling
- **WHEN** the CfApi `rename` callback invokes `CoreOps::rename()` and it returns an error
- **THEN** the system logs the error at `warn` level with source and target paths, does NOT call `ticket.pass()`, and returns `Ok(())` to the proxy

### Requirement: Lossless path handling on Windows
The CfApi backend SHALL handle file paths without lossy Unicode conversion. The `relative_path` helper SHALL return pre-split path components as `OsString` values, preserving any NTFS-legal filenames that contain unpaired UTF-16 surrogates. `CoreOps::resolve_path()` SHALL accept `OsStr` path components and compare them against cache entries using lossless conversion. If an `OsStr` component cannot be converted to a valid `&str`, the lookup SHALL fail gracefully (return `None`) rather than silently corrupting the name.

#### Scenario: Filename with unpaired UTF-16 surrogate
- **WHEN** the CfApi backend receives a callback for a file whose NTFS name contains an unpaired UTF-16 surrogate (valid WTF-16, invalid UTF-8)
- **THEN** the system preserves the `OsString` representation through path resolution
- **AND** the cache lookup fails gracefully (the Graph API cannot store such names, so no match exists)
- **AND** no U+FFFD replacement character is silently substituted into the path

#### Scenario: Normal Unicode filename
- **WHEN** the CfApi backend receives a callback for a file with a standard Unicode name
- **THEN** the `OsStr` component converts losslessly to `&str` and the cache lookup proceeds normally

#### Scenario: FUSE path handling unchanged
- **WHEN** the FUSE backend passes an `OsStr` filename from the kernel to `CoreOps`
- **THEN** the behavior is identical to the current implementation (FUSE filenames are already `OsStr`)

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
The system SHALL cleanly unmount drives without data loss. The `shutdown_on_signal` function SHALL release the mounts mutex before performing blocking unmount operations to prevent deadlock under concurrent access.

#### Scenario: User-initiated unmount
- **WHEN** the user clicks "Unmount" in the tray app
- **THEN** the system flushes all pending writes, waits for in-flight uploads to complete (with a 30-second timeout), unmounts the FUSE filesystem, and confirms unmount to the user

#### Scenario: Forced unmount on shutdown
- **WHEN** the system receives a shutdown signal (SIGTERM, system reboot)
- **THEN** the system flushes pending writes with a 10-second timeout, forcefully unmounts the FUSE filesystem, and saves any unflushed changes to a pending-uploads queue for retry on next start

#### Scenario: shutdown_on_signal releases mutex before unmount
- **WHEN** `shutdown_on_signal` is triggered by a signal
- **THEN** the system drains the mount handles out of the mutex (via `std::mem::take`), releases the mutex lock, then iterates through the handles and unmounts each one sequentially
- **AND** other threads can access the (now-empty) mounts collection during the unmount process

### Requirement: Pending writes flushed on unmount via shared implementation
On unmount, both the FUSE and CfApi backends SHALL flush any pending write-back
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
## Requirements
### Requirement: Atomic inode allocation
The InodeTable SHALL guarantee a 1:1 mapping between `item_id` and `inode` under concurrent access. The `allocate()` method SHALL use a single lock to perform the lookup-or-insert operation atomically — no window SHALL exist between checking for an existing mapping and inserting a new one. All mutating methods (`allocate`, `reassign`, `set_root`, `remove_by_item_id`) SHALL hold a single unified lock covering both the inode-to-item and item-to-inode maps.

#### Scenario: Concurrent allocation for the same item_id
- **WHEN** two threads call `allocate("item123")` simultaneously and no mapping exists yet
- **THEN** exactly one inode SHALL be allocated, and both calls SHALL return the same inode number

#### Scenario: No ghost inode entries after concurrent access
- **WHEN** `allocate()` is called concurrently for the same `item_id`
- **THEN** the `inode_to_item` and `item_to_inode` maps SHALL contain exactly one entry each for that `item_id`, with no orphaned inode numbers

### Requirement: CfApi closed callback skips unmodified files
On Windows, the CfApi `closed()` callback SHALL skip the writeback and upload cycle only when the file can be resolved to a known VFS item and is confirmed unmodified since last sync. Unmodified detection SHALL require a stable metadata match against cached server state (including Last Write Time tolerance and file size consistency for the resolved item). The callback SHALL NOT treat unresolved or non-placeholder local files as unmodified by default.

When the callback cannot safely determine that a file is unmodified (for example unresolved path, missing item mapping, or non-placeholder local file), the system SHALL log the reason and hand off to the Windows local-change ingest path instead of returning silently.

#### Scenario: Read-only file open on Windows
- **WHEN** a user opens a hydrated file in a read-only application and closes it without modification
- **THEN** the `closed()` callback detects a confirmed unmodified state and skips writeback/upload Graph calls

#### Scenario: Modified file close on Windows
- **WHEN** a user edits a placeholder-backed file and saves changes
- **THEN** the `closed()` callback detects modified state and proceeds with writeback and upload

#### Scenario: Closed callback receives unresolved or non-placeholder file
- **WHEN** `closed()` fires for a path that cannot be resolved to a known item or is not placeholder-backed
- **THEN** the callback logs the guard reason and routes the path to local-change ingest handling rather than silently skipping upload

### Requirement: TOCTOU-safe placeholder population on Windows
The system SHALL handle `ERROR_CLOUD_FILE_INVALID_REQUEST` returned by `CfCreatePlaceholders` as a per-item recoverable condition during the `FetchPlaceholders` callback. When a TOCTOU collision is detected, the system SHALL log a `warn!`-level message identifying the item and continue processing remaining items. The system SHALL NOT propagate `ERROR_CLOUD_FILE_INVALID_REQUEST` as a callback error, and SHALL NOT allow such a collision to crash the process. The `FetchPlaceholders` callback SHALL iterate over candidate placeholder items individually so that each item's result can be inspected independently.

#### Scenario: TOCTOU race during placeholder creation
- **WHEN** `CfCreatePlaceholders` returns `ERROR_CLOUD_FILE_INVALID_REQUEST` for an item during the `FetchPlaceholders` callback (because the placeholder was created by another process or thread between the existence check and the API call)
- **THEN** the system logs a `warn!` message identifying the item name and the collision, skips that item, and continues creating placeholders for remaining items without returning an error from the callback

#### Scenario: No TOCTOU race — normal placeholder creation
- **WHEN** `CfCreatePlaceholders` succeeds for an item during the `FetchPlaceholders` callback
- **THEN** the system registers the placeholder and continues to the next item

#### Scenario: Genuine API failure during placeholder creation
- **WHEN** `CfCreatePlaceholders` returns an error other than `ERROR_CLOUD_FILE_INVALID_REQUEST` during the `FetchPlaceholders` callback
- **THEN** the system returns that error from `fetch_placeholders` so the Cloud Files API infrastructure can signal failure to the OS

#### Scenario: Pre-filter removes already-existing items before API call
- **WHEN** the `FetchPlaceholders` callback is invoked for a directory that already has some placeholder files on disk
- **THEN** the system filters out those items before calling `CfCreatePlaceholders`, reducing unnecessary API calls; the per-item error handling acts as a safety net for items that appear between the filter check and the API call

### Requirement: Resilient CfApi callback error handling
Each CfApi callback (`fetch_data`, `fetch_placeholders`, `delete`, `rename`, `closed`, `validate_data`, `state_changed`) SHALL handle errors gracefully without panicking or propagating unhandled exceptions across the FFI boundary. On error, each callback SHALL log sufficient context (callback name, file path, error details) and return `Ok(())` or skip the failing operation rather than returning an error that could trigger Windows error dialogs or cloud-filter panics.

Writeback failures in `closed()` (file read, writeback write, chunk write, finalize, and flush_inode) SHALL emit a `VfsEvent::WritebackFailed { file_name }` event so the UI can notify the user that their changes may not have been saved. This ensures every error path in `closed()` surfaces user feedback.

The CfApi `closed()` callback SHALL only proceed with the writeback cycle when the file has been modified since last sync (see: CfApi closed callback skips unmodified files).

The `CfMountHandle` struct SHALL name the `Connection` field without a leading underscore (i.e., `connection`, not `_connection`) because its drop order relative to `sync_root_id` is safety-critical. The field SHALL be documented to explain that it must be dropped before `sync_root_id` is unregistered.

#### Scenario: fetch_data cannot resolve the file path
- **WHEN** the `fetch_data` callback is invoked for a file whose path cannot be resolved in the cache or via the Graph API
- **THEN** the system logs a warning including the relative path and returns success to the proxy without writing any data to the transfer ticket
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: fetch_data download fails
- **WHEN** the `fetch_data` callback resolves the file successfully but the content download (`read_range_direct`) fails due to a network error or API error
- **THEN** the system logs a warning including the file path and error details and returns success to the proxy without writing any data to the transfer ticket
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: fetch_data write_at fails mid-transfer
- **WHEN** the `fetch_data` callback begins writing hydration data via `ticket.write_at` but a write chunk fails
- **THEN** the system logs a warning including the file path and error details, stops writing further chunks, and returns success to the proxy
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: delete ticket acknowledgement fails
- **WHEN** the `delete` callback has completed its Graph API and cache cleanup but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the delete callback; the cache and Graph API side effects are idempotent

#### Scenario: rename ticket acknowledgement fails
- **WHEN** the `rename` callback has completed its Graph API and cache update but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the rename callback; the cache and Graph API side effects are idempotent

#### Scenario: dehydrate ticket acknowledgement fails
- **WHEN** the `dehydrate` callback has completed its disk cache removal but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the dehydrate callback; the disk cache removal is idempotent

#### Scenario: fetch_data logs path on write_at failure
- **WHEN** `ticket.write_at()` fails during chunked data transfer in `fetch_data`
- **THEN** the callback logs a warning with the file's absolute path (not an undefined variable) and breaks the write loop without panicking

#### Scenario: closed skips unmodified files
- **WHEN** `closed()` fires for a file whose Last Write Time matches the cached server timestamp
- **THEN** the callback returns immediately without reading file content or calling flush_inode

#### Scenario: closed flush_inode failure emits event
- **WHEN** `flush_inode()` returns an error after a successful writeback write in `closed()`
- **THEN** a `VfsEvent::WritebackFailed { file_name }` event is emitted and the error is logged

#### Scenario: CfMountHandle drop order correctness
- **WHEN** a `CfMountHandle` is dropped (either via `unmount()` or implicit drop)
- **THEN** the `connection` field is dropped before `sync_root_id` is unregistered, preventing Windows from rejecting the unregistration due to an active connection

### Requirement: CfApi fetch_data immediate failure signaling
On Windows, the `fetch_data` Cloud Files API callback SHALL signal failure to the operating system immediately on any error, rather than returning without issuing any CfExecute operation. Returning without a CfExecute call leaves Windows waiting until its 60-second internal timeout expires, resulting in error 426 for the requesting process. The callback SHALL resolve the item ID from the placeholder blob set at creation time (`request.file_blob()`), without making any Graph API network call for item resolution.

All `tracing` log calls in `fetch_data` SHALL reference the absolute path variable (`abs_path`) for the file being processed. No undefined variables SHALL appear in log format strings.

#### Scenario: fetch_data — item ID decoded from placeholder blob
- **WHEN** the OS dispatches a `fetch_data` callback for a dehydrated file
- **THEN** the system decodes the item ID from `request.file_blob()` (UTF-8 bytes written at placeholder creation), looks up the corresponding inode in the inode table, and proceeds to hydrate using that inode
- **AND** no Graph API `list_children` or `get_item` call is made to resolve the file path

#### Scenario: fetch_data — blob decode or inode lookup failure
- **WHEN** the placeholder blob is missing, invalid UTF-8, or the decoded item ID has no matching inode in the inode table
- **THEN** the system returns a failure status to the OS immediately (equivalent to `CfExecute` with a non-success `CompletionStatus`)
- **AND** the OS surfaces an error to the requesting process without waiting for any timeout

#### Scenario: fetch_data — download failure
- **WHEN** the Graph API download for the required byte range fails (network error, auth error, HTTP error)
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — empty content returned
- **WHEN** the Graph API returns an empty response body for a non-zero-length file
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — path outside sync root
- **WHEN** the OS dispatches a `fetch_data` callback for a path that is not under the registered sync root
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — write_at failure mid-transfer
- **WHEN** a `write_at` call fails during the chunk transfer loop (e.g., connection closed)
- **THEN** the system aborts the transfer, logs a warning with the absolute file path and error details, and returns a failure status to the OS immediately
- **AND** Windows discards the partial transfer and leaves the file in dehydrated state

### Requirement: CfApi closed callback surfaces upload failures
On Windows, the `closed()` Cloud Files API callback SHALL emit a `VfsEvent::WritebackFailed` event on every error path, including when `flush_inode` fails after a successful writeback write. The system SHALL NOT silently log upload failures without notifying the user.

#### Scenario: flush_inode fails after writeback write succeeds
- **WHEN** the `closed()` callback successfully writes file content to the writeback buffer but the subsequent `flush_inode()` upload fails (network error, auth error, conflict error)
- **THEN** the system logs the error at `error` level and emits a `VfsEvent::WritebackFailed` event with the file name
- **AND** the UI surfaces a notification to the user indicating the file was not uploaded

#### Scenario: writeback write fails
- **WHEN** the `closed()` callback fails to write file content to the writeback buffer
- **THEN** the system logs the error at `error` level, emits a `VfsEvent::WritebackFailed` event, and skips the `flush_inode` call

### Requirement: CfApi writeback failure notification
The system SHALL emit a `VfsEvent::WritebackFailed` event when a CfApi `closed` callback fails to persist file content to the writeback buffer. The app layer SHALL surface this event as a desktop notification informing the user that their changes were not saved.

#### Scenario: Writeback failure emits VfsEvent
- **WHEN** the CfApi `closed` callback fails to write content to the writeback buffer
- **THEN** the system emits a `VfsEvent::WritebackFailed { file_name }` event containing the affected file name

#### Scenario: App surfaces writeback failure notification
- **WHEN** the app layer receives a `VfsEvent::WritebackFailed` event
- **THEN** the system displays a desktop notification: "Failed to save changes to {file_name}. Your edits may be lost."

### Requirement: CfApi state_changed invalidates parent directory cache
On Windows, when the `state_changed()` Cloud Files API callback fires for a path under the sync root, the system SHALL invalidate the changed item's cache entry when resolvable and SHALL invalidate its parent directory cache entry when a parent exists. In addition, for file paths that indicate local mutable content changes, the callback SHALL enqueue local-change ingest evaluation so cache invalidation and upload triggering remain consistent.

#### Scenario: state_changed for a file in a directory
- **WHEN** the OS fires `state_changed` for a file path inside a directory
- **THEN** the system invalidates the file cache entry and parent directory cache entry when resolvable
- **AND** enqueues local-change ingest evaluation for that file path

#### Scenario: state_changed for the sync root itself
- **WHEN** the OS fires `state_changed` for the sync root path
- **THEN** the system invalidates only the sync root cache entry
- **AND** no parent invalidation or file ingest enqueue is performed

#### Scenario: state_changed for an unresolvable path
- **WHEN** the OS fires `state_changed` for a path that cannot be resolved to an inode
- **THEN** the system logs the unresolved-path reason and still performs best-effort local-change ingest evaluation for that path

### Requirement: Windows sync root declares supported in-sync attributes
On Windows, sync root registration SHALL explicitly configure supported in-sync attributes for Cloud Files state evaluation, including last-write-time attributes for files and directories.

#### Scenario: Sync root registration on Windows
- **WHEN** a CfApi mount is registered
- **THEN** the sync root registration includes explicit supported in-sync attributes used by Explorer to determine sync-state transitions

### Requirement: VfsEvent for upload failures
The system SHALL define a `VfsEvent::UploadFailed { file_name: String, reason: String }` variant for generic upload failures. The FUSE backend SHALL emit this event from the `flush` callback when `flush_handle` returns an error, providing parity with the CfApi backend's existing `WritebackFailed` emission on `closed()` errors.

#### Scenario: FUSE flush emits UploadFailed on error
- **WHEN** the FUSE `flush` callback calls `flush_handle` and it returns an error
- **THEN** the system emits `VfsEvent::UploadFailed { file_name, reason }` with the file name and error description
- **AND** returns the appropriate errno to the kernel

#### Scenario: FUSE flush succeeds
- **WHEN** the FUSE `flush` callback calls `flush_handle` and it returns `Ok(())`
- **THEN** no `VfsEvent::UploadFailed` is emitted

### Requirement: VfsEvent for file lock detection
The system SHALL define a `VfsEvent::FileLocked { file_name: String }` variant emitted when a file is detected as locked on OneDrive, either at open time (lock check) or at save time (423 response).

#### Scenario: FileLocked emitted on open
- **WHEN** `open_file` detects that a file is locked via the Graph API response
- **THEN** the system emits `VfsEvent::FileLocked { file_name }` with the file's display name

#### Scenario: FileLocked emitted on 423 Locked upload
- **WHEN** `flush_inode` receives a 423 Locked response and uploads a conflict copy
- **THEN** the system emits `VfsEvent::FileLocked { file_name }` with the original file name

