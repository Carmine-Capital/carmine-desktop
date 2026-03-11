# Capability: Temp File Upload Filter

## Purpose
Prevent transient and system-generated files (Office lock/temp files, OS metadata files) from being uploaded to OneDrive/SharePoint via the Graph API, reducing unnecessary network traffic and avoiding polluting cloud storage with ephemeral local artifacts.

## Requirements

### Requirement: Transient file upload suppression
The system SHALL skip the Graph API upload cycle for files whose names match known transient patterns. When `flush_inode` is called for a file matching a transient pattern, the system SHALL remove the writeback entry without uploading and return success. The file SHALL continue to exist in the local VFS (create, write, read, delete operations SHALL work normally through the in-memory buffer and writeback layer). The system SHALL log a debug-level message when an upload is skipped due to this filter.

The following filename patterns SHALL be treated as transient:
- Names starting with `~$` (Office lock files, e.g. `~$Book1.xlsx`)
- Names starting with `~` and ending with `.tmp` (Office temp files, e.g. `~WRS0001.tmp`)
- `Thumbs.db` (Windows thumbnail cache, case-insensitive)
- `desktop.ini` (Windows folder customization, case-insensitive)
- `.DS_Store` (macOS directory metadata)

The transient check SHALL be a pure function of the filename only (no filesystem state, no item metadata). The check SHALL be applied in `CoreOps::flush_inode` so that both FUSE and WinFsp backends benefit uniformly.

#### Scenario: Office lock file skipped on flush
- **WHEN** a user opens `Report.xlsx` in Excel, causing Excel to create `~$Report.xlsx` in the same directory, and the lock file is flushed
- **THEN** the system skips the Graph API upload for `~$Report.xlsx`, removes its writeback entry, logs a debug message, and returns success
- **AND** the lock file remains accessible locally until Excel deletes it on close

#### Scenario: Office temp file skipped on flush
- **WHEN** Excel creates a temporary file `~WRS0001.tmp` during a save operation and the temp file is flushed
- **THEN** the system skips the Graph API upload for `~WRS0001.tmp`, removes its writeback entry, and returns success

#### Scenario: Windows system file skipped on flush
- **WHEN** Windows Explorer generates `Thumbs.db` or `desktop.ini` in a mounted directory and the file is flushed
- **THEN** the system skips the Graph API upload, removes the writeback entry, and returns success
- **AND** the comparison is case-insensitive (e.g. `THUMBS.DB` is also filtered)

#### Scenario: macOS metadata file skipped on flush
- **WHEN** macOS Finder generates `.DS_Store` in a mounted directory and the file is flushed
- **THEN** the system skips the Graph API upload, removes the writeback entry, and returns success

#### Scenario: Normal file not affected by filter
- **WHEN** a user saves a file named `Budget Report.xlsx` and it is flushed
- **THEN** the system proceeds with the normal upload cycle (conflict check, writeback persist, Graph API upload) without any filtering

#### Scenario: Transient file local operations unaffected
- **WHEN** an application creates, writes to, reads from, or deletes a file matching a transient pattern
- **THEN** all local VFS operations succeed normally (the filter only affects the upload step in `flush_inode`, not create/write/read/delete)
