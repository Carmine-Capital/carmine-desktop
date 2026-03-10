## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: CfApi writeback failure notification
The system SHALL emit a `VfsEvent::WritebackFailed` event when a CfApi `closed` callback fails to persist file content to the writeback buffer. The app layer SHALL surface this event as a desktop notification informing the user that their changes were not saved.

#### Scenario: Writeback failure emits VfsEvent
- **WHEN** the CfApi `closed` callback fails to write content to the writeback buffer
- **THEN** the system emits a `VfsEvent::WritebackFailed { file_name }` event containing the affected file name

#### Scenario: App surfaces writeback failure notification
- **WHEN** the app layer receives a `VfsEvent::WritebackFailed` event
- **THEN** the system displays a desktop notification: "Failed to save changes to {file_name}. Your edits may be lost."
