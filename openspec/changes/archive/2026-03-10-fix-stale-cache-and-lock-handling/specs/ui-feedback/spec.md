## ADDED Requirements

### Requirement: Desktop notification for upload failures
The system SHALL display a desktop notification when a file upload fails on any platform. The app layer SHALL handle `VfsEvent::UploadFailed` events by displaying a notification with the file name and failure reason.

#### Scenario: Upload failure notification displayed
- **WHEN** the app layer receives a `VfsEvent::UploadFailed { file_name, reason }` event
- **THEN** the system displays a desktop notification: "Upload failed for {file_name}: {reason}"

### Requirement: Desktop notification for file lock warning
The system SHALL display a desktop notification when a file is detected as locked on OneDrive. The app layer SHALL handle `VfsEvent::FileLocked` events by displaying a notification informing the user that the file is being edited online and local changes will be saved as a copy.

#### Scenario: File lock warning on open
- **WHEN** the app layer receives a `VfsEvent::FileLocked { file_name }` event during file open
- **THEN** the system displays a desktop notification: "{file_name} is being edited online. Local changes will be saved as a separate copy."

#### Scenario: File lock notification on save
- **WHEN** the app layer receives a `VfsEvent::FileLocked { file_name }` event during a save (423 Locked)
- **THEN** the system displays a desktop notification: "{file_name} is locked online. Your changes were saved as a conflict copy."
