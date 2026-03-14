## MODIFIED Requirements

### Requirement: WinFsp FileSystemContext implementation
On Windows, the system SHALL implement the WinFsp `FileSystemContext` trait in a `carminedesktopWinFsp` struct that delegates all filesystem operations to `CoreOps`. The `FileSystemContext::FileContext` associated type SHALL be a `WinFspFileContext` struct containing the resolved inode number (`ino: u64`), an optional CoreOps file handle (`fh: Option<u64>`), and a directory flag (`is_dir: bool`). The struct SHALL hold an `Arc<CoreOps>` instance and a `tokio::runtime::Handle` for bridging async operations via `rt.block_on()`.

The `open` callback SHALL extract the caller process ID using `winfsp_sys::FspFileSystemOperationProcessIdF()` (an `unsafe` FFI call valid during Create/Open callbacks) and pass it to `CoreOps::open_file()` as `Some(pid)`. If the function returns 0, the callback SHALL pass `None`.

#### Scenario: carminedesktopWinFsp created with CoreOps
- **WHEN** a WinFsp filesystem is initialized for a mounted drive
- **THEN** the system creates a `carminedesktopWinFsp` instance holding a shared `CoreOps`, a Tokio runtime handle, and a shared `OpenFileTable` reference

#### Scenario: FileContext returned on open
- **WHEN** WinFsp dispatches an `open` callback for a file
- **THEN** the system extracts the caller PID via `FspFileSystemOperationProcessIdF()`
- **AND** passes `Some(pid)` (or `None` if PID is 0) to `CoreOps::open_file(ino, caller_pid, file_path)`
- **AND** returns a `WinFspFileContext` with the resolved inode, a CoreOps file handle, and `is_dir: false`

#### Scenario: FileContext returned on directory open
- **WHEN** WinFsp dispatches an `open` callback for a directory
- **THEN** the system returns a `WinFspFileContext` with the resolved inode, `fh: None`, and `is_dir: true`

### Requirement: WinFsp error mapping
The system SHALL map `VfsError` variants to WinFsp NTSTATUS codes:
- `VfsError::NotFound` -> `STATUS_OBJECT_NAME_NOT_FOUND`
- `VfsError::NotADirectory` -> `STATUS_NOT_A_DIRECTORY`
- `VfsError::DirectoryNotEmpty` -> `STATUS_DIRECTORY_NOT_EMPTY`
- `VfsError::PermissionDenied` -> `STATUS_ACCESS_DENIED`
- `VfsError::CollabRedirect` -> `STATUS_CANCELLED`
- `VfsError::TimedOut` -> `STATUS_IO_TIMEOUT`
- `VfsError::QuotaExceeded` -> `STATUS_DISK_FULL`
- `VfsError::IoError(_)` -> `STATUS_IO_DEVICE_ERROR`

`STATUS_CANCELLED` (0xC0000120) maps to Win32 `ERROR_OPERATION_ABORTED`. This causes applications to treat the failed open as an intentionally cancelled operation rather than an access-denied condition, avoiding retry loops and "file locked" dialogs in Office applications.

#### Scenario: CoreOps returns NotFound
- **WHEN** a CoreOps operation returns `VfsError::NotFound`
- **THEN** the WinFsp callback returns `STATUS_OBJECT_NAME_NOT_FOUND` and Windows surfaces a "file not found" error to the application

#### Scenario: CoreOps returns IoError
- **WHEN** a CoreOps operation returns `VfsError::IoError` (e.g., Graph API network failure)
- **THEN** the WinFsp callback returns `STATUS_IO_DEVICE_ERROR` and Windows surfaces an I/O error to the application

#### Scenario: CoreOps returns CollabRedirect
- **WHEN** a CoreOps operation returns `VfsError::CollabRedirect` (CollabGate chose online open)
- **THEN** the WinFsp callback returns `STATUS_CANCELLED`
- **AND** the calling application receives `ERROR_OPERATION_ABORTED` from `GetLastError()`
