## ADDED Requirements

### Requirement: Detect file lock status on open
The system SHALL check the lock status of a file when `open_file` is called, using metadata returned by the Graph API `get_item` response. When the file is locked (e.g. co-authoring session active, SharePoint checkout), the system SHALL emit a `VfsEvent::FileLocked` event with the file name. The open SHALL proceed normally — lock detection is informational, not blocking.

#### Scenario: File is locked on OneDrive during open
- **WHEN** `open_file` is called for a file and the Graph API `get_item` response indicates the file is locked (e.g. `publication` facet shows `versionId` is checked out, or co-authoring lock is active)
- **THEN** the system emits `VfsEvent::FileLocked { file_name }` with the file's display name
- **AND** the open proceeds normally, returning a valid file handle

#### Scenario: File is not locked during open
- **WHEN** `open_file` is called for a file and the Graph API `get_item` response does not indicate any lock
- **THEN** no `VfsEvent::FileLocked` is emitted
- **AND** the open proceeds normally

#### Scenario: Lock check fails due to network error
- **WHEN** the `get_item` call fails during `open_file` (network error, timeout)
- **THEN** the system does not emit `VfsEvent::FileLocked`
- **AND** the open proceeds using cached metadata (existing fallback behavior)

### Requirement: Conflict copy on 423 Locked upload
When `flush_inode` receives a 423 Locked error from the Graph API upload, the system SHALL upload the local content as a conflict copy to the same parent folder using the existing `conflict_name()` function (e.g. `report.conflict.1741612345.xlsx`). After the conflict copy upload succeeds, the system SHALL remove the original entry from the writeback buffer and emit `VfsEvent::FileLocked` with the file name.

#### Scenario: Upload fails with 423 Locked
- **WHEN** `flush_inode` attempts to upload file content and the Graph API returns 423 Locked
- **THEN** the system uploads the content as a conflict copy named using `conflict_name()` to the same parent folder
- **AND** emits `VfsEvent::FileLocked { file_name }` with the original file name
- **AND** removes the entry from the writeback buffer
- **AND** returns `Err(VfsError::IoError)` indicating the file was locked

#### Scenario: Conflict copy upload also fails
- **WHEN** `flush_inode` receives 423 Locked and the conflict copy upload also fails
- **THEN** the system logs the error at `error` level
- **AND** the writeback buffer entry is NOT removed (content preserved for crash recovery)
- **AND** returns `Err(VfsError::IoError)` with the failure reason

#### Scenario: Multiple saves while file is locked
- **WHEN** a user saves a file locally multiple times while it remains locked on OneDrive
- **THEN** each save creates a separate conflict copy with a unique timestamp in the name
- **AND** each save emits a `VfsEvent::FileLocked` event

### Requirement: Distinct error for 423 Locked HTTP response
The Graph client SHALL map HTTP 423 Locked responses to a distinct `Error::Locked` variant in `carminedesktop_core::Error`, analogous to the existing `Error::PreconditionFailed` for HTTP 412. This allows callers to match on the specific error type and handle locked files differently from generic API errors.

#### Scenario: Graph API returns 423
- **WHEN** the Graph client receives a 423 status code from any API call
- **THEN** it returns `Err(carminedesktop_core::Error::Locked)`

#### Scenario: Graph API returns other 4xx errors
- **WHEN** the Graph client receives a 4xx status code other than 412 or 423
- **THEN** existing error handling behavior is unchanged
