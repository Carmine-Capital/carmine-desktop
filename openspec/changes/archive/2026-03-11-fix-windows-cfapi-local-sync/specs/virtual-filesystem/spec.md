## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Windows sync root declares supported in-sync attributes
On Windows, sync root registration SHALL explicitly configure supported in-sync attributes for Cloud Files state evaluation, including last-write-time attributes for files and directories.

#### Scenario: Sync root registration on Windows
- **WHEN** a CfApi mount is registered
- **THEN** the sync root registration includes explicit supported in-sync attributes used by Explorer to determine sync-state transitions
