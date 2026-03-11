## ADDED Requirements

### Requirement: Convert uploaded local files to CfApi placeholders
On Windows, after a `local:*` file is successfully uploaded to OneDrive via `flush_inode()`, the system SHALL convert the local file to a CfApi placeholder. The conversion SHALL use `Placeholder::convert_to_placeholder()` with the server-assigned item ID as the file blob, and SHALL call `mark_in_sync()` to set the placeholder's sync state. The conversion SHALL NOT modify the file's content on disk.

#### Scenario: Successful upload of local file
- **WHEN** `flush_inode()` successfully uploads a file whose item ID starts with `local:` and receives a server-assigned item ID in return
- **THEN** the system converts the file at its absolute path to a CfApi placeholder with the server item ID as the blob, marks it in-sync, and the file appears with the synced overlay icon in Explorer

#### Scenario: File modified between upload and conversion
- **WHEN** a user modifies a file after upload completes but before placeholder conversion executes
- **THEN** the conversion proceeds (it only updates NTFS reparse point metadata, not file content), and the filesystem watcher detects the subsequent modification, triggering a new ingest and upload cycle

#### Scenario: Conversion fails
- **WHEN** `Placeholder::convert_to_placeholder()` fails (e.g., file locked by another process, permission error)
- **THEN** the system logs a warning with the file path and error details and continues without retrying the conversion
- **AND** the file remains a regular NTFS file; future modifications are still detected by the filesystem watcher and synced via the ingest pipeline

#### Scenario: File already a placeholder
- **WHEN** the system attempts to convert a file that is already a CfApi placeholder (e.g., a file that was hydrated and modified in place)
- **THEN** the item ID started with a server-assigned ID (not `local:`), so the conversion path is not triggered

### Requirement: Placeholder conversion is Windows-only
The post-upload placeholder conversion logic SHALL be gated with `#[cfg(target_os = "windows")]`. On Linux and macOS, `flush_inode()` SHALL not attempt any placeholder conversion.

#### Scenario: Compilation on Linux or macOS
- **WHEN** the codebase is compiled for Linux or macOS
- **THEN** the placeholder conversion code is not included in the binary and `flush_inode()` behavior is unchanged
