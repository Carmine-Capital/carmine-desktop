## Purpose

This spec defines the WinFsp (Windows File System Proxy) filesystem implementation for CloudMount on Windows, providing native filesystem access to Microsoft 365 drives.

## Requirements

### Requirement: WinFsp FileSystemContext implementation
On Windows, the system SHALL implement the WinFsp `FileSystemContext` trait in a `CloudMountWinFsp` struct that delegates all filesystem operations to `CoreOps`. The `FileSystemContext::FileContext` associated type SHALL be a `WinFspFileContext` struct containing the resolved inode number (`ino: u64`), an optional CoreOps file handle (`fh: Option<u64>`), and a directory flag (`is_dir: bool`). The struct SHALL hold an `Arc<CoreOps>` instance and a `tokio::runtime::Handle` for bridging async operations via `rt.block_on()`.

#### Scenario: CloudMountWinFsp created with CoreOps
- **WHEN** a WinFsp filesystem is initialized for a mounted drive
- **THEN** the system creates a `CloudMountWinFsp` instance holding a shared `CoreOps`, a Tokio runtime handle, and a shared `OpenFileTable` reference

#### Scenario: FileContext returned on open
- **WHEN** WinFsp dispatches an `open` callback for a file
- **THEN** the system returns a `WinFspFileContext` with the resolved inode, a CoreOps file handle from `CoreOps::open_file(ino)`, and `is_dir: false`

#### Scenario: FileContext returned on directory open
- **WHEN** WinFsp dispatches an `open` callback for a directory
- **THEN** the system returns a `WinFspFileContext` with the resolved inode, `fh: None`, and `is_dir: true`

### Requirement: WinFsp path-to-inode resolution
The system SHALL resolve WinFsp path strings to inode numbers using `CoreOps::resolve_path()`. The `get_security_by_name()` callback SHALL split the incoming `U16CStr` path on backslash separators, convert each component to an `OsString`, and pass the components to `CoreOps::resolve_path()`. If resolution succeeds, the system SHALL return a `FileSecurity` with the file's attributes. If resolution fails (item not found), the system SHALL return `STATUS_OBJECT_NAME_NOT_FOUND`.

The root path (`\`) SHALL resolve to ROOT_INODE (1) without calling `resolve_path`.

#### Scenario: Resolve nested file path
- **WHEN** `get_security_by_name` receives the path `\Documents\Reports\quarterly.xlsx`
- **THEN** the system splits on `\`, calls `CoreOps::resolve_path(["Documents", "Reports", "quarterly.xlsx"])`, and returns the file's attributes (size, timestamps, `FILE_ATTRIBUTE_NORMAL`)

#### Scenario: Resolve directory path
- **WHEN** `get_security_by_name` receives the path `\Documents\Reports`
- **THEN** the system resolves the path and returns attributes with `FILE_ATTRIBUTE_DIRECTORY`

#### Scenario: Resolve root path
- **WHEN** `get_security_by_name` receives the path `\`
- **THEN** the system returns attributes for ROOT_INODE with `FILE_ATTRIBUTE_DIRECTORY` without calling `resolve_path`

#### Scenario: Path not found
- **WHEN** `get_security_by_name` receives a path that does not exist in the inode table or cache
- **THEN** the system returns `STATUS_OBJECT_NAME_NOT_FOUND`

#### Scenario: Path with Unicode characters
- **WHEN** `get_security_by_name` receives a path containing non-ASCII Unicode characters (e.g., CJK, emoji, accented characters)
- **THEN** the system converts the `U16CStr` to `OsString` losslessly and resolves normally

### Requirement: WinFsp file attribute mapping
The system SHALL map `DriveItem` metadata to WinFsp `FileInfo` fields as follows:
- `file_size`: `DriveItem.size` (as `u64`) for files, 0 for directories
- `allocation_size`: `file_size` rounded up to the nearest 4096-byte boundary
- `file_attributes`: `FILE_ATTRIBUTE_DIRECTORY` for folders, `FILE_ATTRIBUTE_NORMAL` for files
- `creation_time`: `DriveItem.created` converted to Windows FILETIME (100-nanosecond intervals since 1601-01-01)
- `last_access_time`: same as `last_write_time`
- `last_write_time`: `DriveItem.last_modified` converted to Windows FILETIME
- `change_time`: same as `last_write_time`

If a timestamp field is `None`, the system SHALL use the Windows epoch (1601-01-01 00:00:00 UTC) as the default.

#### Scenario: Map file DriveItem to FileInfo
- **WHEN** `get_file_info` is called for a file context with inode resolving to a DriveItem of size 2048 bytes
- **THEN** the system returns `file_size: 2048`, `allocation_size: 4096`, `file_attributes: FILE_ATTRIBUTE_NORMAL`, and timestamps converted from the DriveItem's UTC `DateTime` fields to Windows FILETIME format

#### Scenario: Map directory DriveItem to FileInfo
- **WHEN** `get_file_info` is called for a directory context
- **THEN** the system returns `file_size: 0`, `allocation_size: 0`, `file_attributes: FILE_ATTRIBUTE_DIRECTORY`, and timestamps from the DriveItem

#### Scenario: DriveItem with missing timestamps
- **WHEN** a DriveItem has `last_modified: None` and `created: None`
- **THEN** the system uses the Windows epoch (1601-01-01) for all timestamp fields in the FileInfo

#### Scenario: Open file handle overrides cached size
- **WHEN** `get_file_info` is called for a file that has an open handle with content of N bytes, and the cached DriveItem reports a different size
- **THEN** the system returns `file_size: N` from the open handle's content buffer (same as FUSE `getattr` behavior)

### Requirement: WinFsp error mapping
The system SHALL map `VfsError` variants to WinFsp NTSTATUS codes:
- `VfsError::NotFound` -> `STATUS_OBJECT_NAME_NOT_FOUND`
- `VfsError::NotADirectory` -> `STATUS_NOT_A_DIRECTORY`
- `VfsError::DirectoryNotEmpty` -> `STATUS_DIRECTORY_NOT_EMPTY`
- `VfsError::PermissionDenied` -> `STATUS_ACCESS_DENIED`
- `VfsError::TimedOut` -> `STATUS_IO_TIMEOUT`
- `VfsError::QuotaExceeded` -> `STATUS_DISK_FULL`
- `VfsError::IoError(_)` -> `STATUS_IO_DEVICE_ERROR`

#### Scenario: CoreOps returns NotFound
- **WHEN** a CoreOps operation returns `VfsError::NotFound`
- **THEN** the WinFsp callback returns `STATUS_OBJECT_NAME_NOT_FOUND` and Windows surfaces a "file not found" error to the application

#### Scenario: CoreOps returns IoError
- **WHEN** a CoreOps operation returns `VfsError::IoError` (e.g., Graph API network failure)
- **THEN** the WinFsp callback returns `STATUS_IO_DEVICE_ERROR` and Windows surfaces an I/O error to the application

### Requirement: WinFsp file read operations
The system SHALL implement the `read` callback by delegating to `CoreOps::read_handle(fh, offset, size)`. The `fh` value SHALL come from the `WinFspFileContext`. The system SHALL copy the returned bytes into the WinFsp-provided buffer and return the number of bytes read.

#### Scenario: Read from fully loaded file
- **WHEN** WinFsp dispatches a `read` callback with offset 0 and size 4096 for a file handle whose content is fully downloaded
- **THEN** the system calls `CoreOps::read_handle(fh, 0, 4096)`, copies the result into the WinFsp buffer, and returns the byte count

#### Scenario: Read from streaming file
- **WHEN** WinFsp dispatches a `read` callback for a file handle whose content is still being downloaded
- **THEN** `CoreOps::read_handle` blocks until the requested byte range is available, then returns the data

#### Scenario: Read beyond end of file
- **WHEN** WinFsp dispatches a `read` callback with an offset beyond the file's size
- **THEN** the system returns 0 bytes read

### Requirement: WinFsp file write operations
The system SHALL implement the `write` callback by delegating to `CoreOps::write_handle(fh, offset, data)`. If `write_to_eof` is true, the system SHALL determine the current file size from the open handle and set offset to end-of-file. The system SHALL update the `FileInfo` output parameter with the new file size after writing.

#### Scenario: Write at specific offset
- **WHEN** WinFsp dispatches a `write` callback with data at offset 1024
- **THEN** the system calls `CoreOps::write_handle(fh, 1024, data)`, updates `file_info.file_size` to reflect the new size, and returns the number of bytes written

#### Scenario: Write to end of file
- **WHEN** WinFsp dispatches a `write` callback with `write_to_eof: true`
- **THEN** the system determines the current end-of-file position from the handle's content size, calls `CoreOps::write_handle(fh, eof_offset, data)`, and returns the bytes written

#### Scenario: Write to file with in-progress download
- **WHEN** WinFsp dispatches a `write` for a file handle whose content is still streaming
- **THEN** `CoreOps::write_handle` blocks until the download completes, then performs the write

### Requirement: WinFsp cleanup and close semantics
The system SHALL implement both `cleanup` and `close` callbacks. The `cleanup` callback fires when the last user handle is closed (but the object may still be referenced by the OS). The `close` callback fires when the object is being destroyed. The `cleanup` callback SHALL flush dirty file handles by calling `CoreOps::flush_handle(fh)`. If the flush fails, the system SHALL emit a `VfsEvent::UploadFailed` event. The `close` callback SHALL release the CoreOps file handle by calling `CoreOps::release_file(fh)`.

For directories, both `cleanup` and `close` SHALL be no-ops (directories have no file handle to flush or release).

#### Scenario: Cleanup flushes dirty file
- **WHEN** WinFsp dispatches a `cleanup` callback for a file handle with pending writes
- **THEN** the system calls `CoreOps::flush_handle(fh)` to flush content to the writeback buffer and upload to the Graph API

#### Scenario: Cleanup flush failure emits event
- **WHEN** `CoreOps::flush_handle(fh)` fails during cleanup (network error, auth error)
- **THEN** the system emits `VfsEvent::UploadFailed { file_name, reason }` and logs the error
- **AND** the file remains in the writeback buffer for retry

#### Scenario: Close releases file handle
- **WHEN** WinFsp dispatches a `close` callback for a file context
- **THEN** the system calls `CoreOps::release_file(fh)` to free the content buffer from the OpenFileTable

#### Scenario: Cleanup and close for directory
- **WHEN** WinFsp dispatches `cleanup` or `close` for a directory context
- **THEN** the system takes no action (directories have `fh: None`)

#### Scenario: Delete on cleanup
- **WHEN** WinFsp dispatches a `cleanup` callback with the delete flag set
- **THEN** the system calls `CoreOps::unlink(parent_ino, name)` for files or `CoreOps::rmdir(parent_ino, name)` for directories before releasing the handle

### Requirement: WinFsp directory listing
The system SHALL implement the `read_directory` callback by calling `CoreOps::list_children(ino)` and encoding each child as a `DirInfo` entry in the WinFsp buffer. The listing SHALL include `.` and `..` entries. Each entry SHALL include full file attributes (size, timestamps, type). If the buffer fills before all entries are written, the system SHALL stop and WinFsp will issue follow-up requests using the marker.

#### Scenario: List directory contents
- **WHEN** WinFsp dispatches a `read_directory` callback for a directory
- **THEN** the system calls `CoreOps::list_children(ino)`, encodes `.` and `..` entries followed by each child's name and attributes as `DirInfo` entries, and returns the byte count written to the buffer

#### Scenario: Large directory exceeds buffer
- **WHEN** a directory has more entries than fit in a single WinFsp buffer
- **THEN** the system fills the buffer with as many entries as possible and returns; WinFsp issues a follow-up request with a marker pointing to the last returned entry

#### Scenario: Empty directory
- **WHEN** WinFsp dispatches `read_directory` for a directory with no children
- **THEN** the system returns only `.` and `..` entries

### Requirement: WinFsp file and directory creation
The system SHALL implement the `create` callback. For files (`FILE_DIRECTORY_FILE` not set in `create_options`), the system SHALL call `CoreOps::create_file(parent_ino, name)` and return a `WinFspFileContext` with the new inode and file handle. For directories (`FILE_DIRECTORY_FILE` set), the system SHALL call `CoreOps::mkdir(parent_ino, name)` and return a `WinFspFileContext` with `fh: None`.

#### Scenario: Create new file
- **WHEN** WinFsp dispatches a `create` callback for a new file
- **THEN** the system resolves the parent directory inode from the path, calls `CoreOps::create_file(parent_ino, name)`, populates the FileInfo with the new item's attributes, and returns a `WinFspFileContext` with the new inode and file handle

#### Scenario: Create new directory
- **WHEN** WinFsp dispatches a `create` callback with `FILE_DIRECTORY_FILE` in create_options
- **THEN** the system resolves the parent directory inode, calls `CoreOps::mkdir(parent_ino, name)`, populates the FileInfo, and returns a `WinFspFileContext` with `fh: None` and `is_dir: true`

### Requirement: WinFsp delete operations
The system SHALL implement the `set_delete` callback. When `delete_file` is true, the system SHALL defer the actual deletion to the `cleanup` callback (where the delete flag is checked). This follows WinFsp semantics: `set_delete` marks the intent, `cleanup` executes it.

#### Scenario: File marked for deletion
- **WHEN** WinFsp calls `set_delete` with `delete_file: true` for a file
- **THEN** the system records the deletion intent; the actual `CoreOps::unlink()` call happens in the subsequent `cleanup` callback

#### Scenario: Directory marked for deletion
- **WHEN** WinFsp calls `set_delete` with `delete_file: true` for a non-empty directory
- **THEN** the system SHALL return `STATUS_DIRECTORY_NOT_EMPTY`

### Requirement: WinFsp rename operations
The system SHALL implement the `rename` callback by resolving source and destination parent inodes from the paths and delegating to `CoreOps::rename(src_parent_ino, src_name, dst_parent_ino, dst_name)`. If `replace_if_exists` is false and the destination exists, the system SHALL return `STATUS_OBJECT_NAME_COLLISION`.

#### Scenario: Rename file in same directory
- **WHEN** WinFsp dispatches a `rename` callback with source `\docs\old.txt` and destination `\docs\new.txt`
- **THEN** the system resolves both parent inodes (same directory), calls `CoreOps::rename(parent_ino, "old.txt", parent_ino, "new.txt")`, and returns success

#### Scenario: Move file to different directory
- **WHEN** WinFsp dispatches a `rename` with source `\docs\file.txt` and destination `\archive\file.txt`
- **THEN** the system resolves both parent inodes (different directories) and calls `CoreOps::rename(docs_ino, "file.txt", archive_ino, "file.txt")`

#### Scenario: Rename with replace_if_exists false and destination exists
- **WHEN** the destination file already exists and `replace_if_exists` is false
- **THEN** the system returns `STATUS_OBJECT_NAME_COLLISION` without calling CoreOps

### Requirement: WinFsp volume information
The system SHALL implement the `get_volume_info` callback by calling `CoreOps::get_quota()` to retrieve drive capacity and remaining space. The volume label SHALL be "CloudMount" and the filesystem name SHALL be "cloudmount".

#### Scenario: Volume info with quota available
- **WHEN** WinFsp dispatches a `get_volume_info` callback and the drive has quota information
- **THEN** the system returns `total_size` and `free_size` from `CoreOps::get_quota()` with volume label "CloudMount"

#### Scenario: Volume info without quota
- **WHEN** `CoreOps::get_quota()` returns `None` (quota not yet fetched)
- **THEN** the system returns a large fallback value (1 TB total, 1 TB free) so applications do not refuse writes

### Requirement: WinFsp set_basic_info and set_file_size
The system SHALL implement `set_basic_info` as a no-op that returns the current file attributes (CloudMount does not support setting timestamps or attributes from the client). The system SHALL implement `set_file_size` by delegating to `CoreOps::truncate(ino, new_size)`.

#### Scenario: Application sets file timestamps
- **WHEN** WinFsp dispatches `set_basic_info` with new timestamps
- **THEN** the system ignores the requested timestamps and returns the current FileInfo from cache (server timestamps are authoritative)

#### Scenario: Application truncates file
- **WHEN** WinFsp dispatches `set_file_size` with a new size
- **THEN** the system calls `CoreOps::truncate(ino, new_size)` and returns updated FileInfo

### Requirement: WinFsp overwrite operation
The system SHALL implement the `overwrite` callback by truncating the existing file content to zero bytes via `CoreOps::truncate(ino, 0)` and returning updated FileInfo. This handles the Windows `CREATE_ALWAYS` / `TRUNCATE_EXISTING` open dispositions.

#### Scenario: File overwritten on open
- **WHEN** WinFsp dispatches an `overwrite` callback for an existing file
- **THEN** the system calls `CoreOps::truncate(ino, 0)` to clear the content, populates FileInfo with size 0, and returns the existing `WinFspFileContext`

### Requirement: WinFsp mount handle lifecycle
The system SHALL provide a `WinFspMountHandle` struct with the same public API surface as the FUSE `MountHandle`: `mount()`, `unmount()`, `drive_id() -> &str`, `mountpoint() -> &str`, and `delta_observer() -> Arc<dyn DeltaSyncObserver>`.

`mount()` SHALL:
1. Fetch the drive root item from the Graph API and seed it into caches as ROOT_INODE
2. Create a `CloudMountWinFsp` filesystem context
3. Create a `WinFspDeltaObserver` sharing the `OpenFileTable`
4. Configure `VolumeParams` with filesystem name "cloudmount" and `FileInfoTimeout` of 5000ms
5. Create a `FileSystemHost`, mount it at the configured path, and start it
6. Return the `WinFspMountHandle`

`unmount()` SHALL flush pending writes via the shared `flush_pending` function, stop the `FileSystemHost`, and unmount.

#### Scenario: Mount drive via WinFsp
- **WHEN** `WinFspMountHandle::mount()` is called with a valid drive_id and mountpoint
- **THEN** the system fetches the root item, seeds caches, creates the WinFsp filesystem host, mounts at the specified directory path, starts the host, and returns a mount handle with a delta observer

#### Scenario: Mount at directory path
- **WHEN** `mount()` is called with a directory path like `C:\Users\user\Cloud\OneDrive`
- **THEN** WinFsp mounts the filesystem at that directory and it appears as a regular folder in Explorer

#### Scenario: Mount at drive letter
- **WHEN** `mount()` is called with a drive letter path like `Z:`
- **THEN** WinFsp mounts the filesystem as drive Z: and it appears as a drive in Explorer

#### Scenario: Mount fails when WinFsp not installed
- **WHEN** `mount()` is called but the WinFsp DLL cannot be loaded
- **THEN** the system returns a `Filesystem` error indicating WinFsp is not available

#### Scenario: Unmount flushes pending writes
- **WHEN** `unmount()` is called on a mount with pending writeback entries
- **THEN** the system calls the shared `flush_pending` function, stops the FileSystemHost, unmounts the volume, and returns success

#### Scenario: Root resolution failure
- **WHEN** `mount()` cannot fetch the drive root item from the Graph API
- **THEN** the mount fails with an error and no WinFsp filesystem is created

### Requirement: WinFsp delta sync observer
The system SHALL implement `DeltaSyncObserver` for WinFsp via a `WinFspDeltaObserver` struct. The observer SHALL hold a shared reference to the `OpenFileTable`. When `on_inode_content_changed(ino)` is called, the observer SHALL call `OpenFileTable::mark_stale_by_ino(ino)` to mark all open handles for that inode as stale, so the next `read()` re-downloads content.

#### Scenario: Delta sync detects remote content change
- **WHEN** delta sync calls `on_inode_content_changed(42)` for a file with inode 42 that has open handles
- **THEN** the observer marks all handles for inode 42 as stale via `mark_stale_by_ino(42)`
- **AND** the next `read()` call on those handles re-downloads fresh content from the Graph API

#### Scenario: Delta sync for file with no open handles
- **WHEN** delta sync calls `on_inode_content_changed(42)` for a file with no open handles
- **THEN** the observer calls `mark_stale_by_ino(42)` which finds no matching handles and returns
- **AND** the next `open()` call for that inode will load fresh content via the dirty-inode check in CoreOps

#### Scenario: Observer created before mount
- **WHEN** `WinFspMountHandle::mount()` creates the filesystem
- **THEN** it creates the `WinFspDeltaObserver` sharing the same `OpenFileTable` as the `CloudMountWinFsp` instance, and the observer is accessible via `delta_observer()`

### Requirement: WinFsp driver availability check
On Windows, the `preflight_checks()` function SHALL verify WinFsp driver availability. The check SHALL query the registry key `HKLM\SOFTWARE\WinFsp\InstallDir`. If the key exists, the system SHALL verify the WinFsp DLL (`winfsp-x64.dll` on 64-bit systems) is present in the install directory's `bin` subdirectory. If either check fails, the function SHALL return an error with a message directing the user to install WinFsp.

#### Scenario: WinFsp installed and available
- **WHEN** preflight checks run on Windows and the WinFsp registry key exists and the DLL is present
- **THEN** the check passes and the application continues normally

#### Scenario: WinFsp not installed
- **WHEN** preflight checks run on Windows and the WinFsp registry key does not exist
- **THEN** the check fails with the error message "WinFsp is required but not installed. Download it from https://winfsp.dev/"

#### Scenario: WinFsp registry key exists but DLL missing
- **WHEN** the registry key exists but the DLL file is not found at the expected path
- **THEN** the check fails with an error message indicating a broken WinFsp installation

> **Historical note:** CfApi sync root cleanup was performed as a one-time migration step in v0.x. The `cfapi_migrated` config flag and `cleanup_cfapi_sync_roots()` function were removed once migration was complete.

### Requirement: Windows headless mode support
On Windows with the WinFsp backend, the system SHALL support `--headless` mode. The `run_headless()` function SHALL NOT reject Windows builds. WinFsp mounts SHALL work without a desktop session because `FileSystemHost::start()` operates independently of Explorer or any GUI components.

#### Scenario: Headless mode on Windows
- **WHEN** the user runs `cloudmount --headless` on Windows
- **THEN** the system initializes WinFsp mounts, starts delta sync, and runs until terminated by a signal
- **AND** the mounted drives are accessible by all processes on the system

#### Scenario: Headless mode graceful shutdown
- **WHEN** a headless Windows instance receives Ctrl+C or SIGTERM
- **THEN** the system flushes pending writes, stops WinFsp hosts, unmounts all drives, and exits cleanly
