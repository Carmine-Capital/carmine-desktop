## ADDED Requirements

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
