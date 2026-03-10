## ADDED Requirements

### Requirement: Placeholder metadata update after delta sync
On Windows, the system SHALL update CfApi NTFS placeholder metadata when delta sync detects that a remote file's content has changed (eTag mismatch). The update SHALL set the placeholder's size, timestamps, and file blob to match the new `DriveItem` metadata, dehydrate the placeholder so the next user access triggers a fresh `fetch_data()`, and mark the placeholder as in-sync. The update SHALL be performed as a single atomic `CfUpdatePlaceholder` call combining metadata, dehydration, and sync state flags.

#### Scenario: Remote file content changed
- **WHEN** delta sync detects that a file's eTag has changed on the server
- **THEN** the system opens the placeholder at the file's mount-relative path, updates its metadata (size, last modified time, created time) to match the new `DriveItem`, dehydrates the placeholder content, updates the file blob to the item ID, marks it as in-sync, and closes the placeholder handle
- **AND** the next time a user or application accesses the file, Windows dispatches a `fetch_data` callback requesting the correct (new) byte range

#### Scenario: Remote folder metadata changed
- **WHEN** delta sync detects that a folder's eTag has changed on the server
- **THEN** the system opens the placeholder at the folder's mount-relative path and updates its metadata (timestamps) without dehydration (folders have no content to dehydrate), and marks it as in-sync

#### Scenario: Placeholder update fails with sharing violation
- **WHEN** the system attempts to update a placeholder that is currently open by another process (sharing violation or oplock conflict)
- **THEN** the system logs a warning with the file path and error details, skips the update for that item, and continues processing remaining items
- **AND** the item remains marked dirty in the cache so `open_file` will download fresh content regardless, and the next delta sync cycle will retry the placeholder update

#### Scenario: Placeholder file does not exist on disk
- **WHEN** the system attempts to update a placeholder for an item whose file does not exist on disk (e.g., parent directory never browsed, placeholder never created)
- **THEN** the system skips the update for that item without error (no placeholder to update; one will be created when the parent directory is next browsed via `fetch_placeholders`)

#### Scenario: Item has pending writeback
- **WHEN** delta sync detects a remote content change for a file that has pending local writes in the writeback buffer
- **THEN** the system SHALL skip the placeholder dehydration for that item and log a warning indicating the conflict
- **AND** the writeback upload will proceed and handle conflict detection via the existing eTag-based conflict resolution mechanism

### Requirement: Placeholder deletion after delta sync
On Windows, the system SHALL remove CfApi NTFS placeholders from the mount directory when delta sync detects that items have been deleted on the server. The deletion SHALL use standard filesystem removal APIs (`std::fs::remove_file` for files, `std::fs::remove_dir` for empty folders).

#### Scenario: Remote file deleted
- **WHEN** delta sync detects that a file has been deleted on the server
- **THEN** the system removes the placeholder file at the file's mount-relative path using `std::fs::remove_file`
- **AND** the file no longer appears in Explorer's directory listing

#### Scenario: Remote folder deleted
- **WHEN** delta sync detects that an empty folder has been deleted on the server
- **THEN** the system removes the placeholder directory at the folder's mount-relative path using `std::fs::remove_dir`

#### Scenario: Delete fails because file is in use
- **WHEN** the system attempts to remove a placeholder that is currently open by another process
- **THEN** the system logs a warning with the file path and error details, skips the deletion for that item, and continues processing remaining items
- **AND** the item is already removed from all caches, so it will not appear in subsequent `fetch_placeholders` results; the stale placeholder will be cleaned up when the user or OS releases the file

#### Scenario: Placeholder already absent
- **WHEN** the system attempts to remove a placeholder for a deleted item but the file does not exist on disk
- **THEN** the system treats this as a no-op success (the desired state — file absent — is already achieved)

#### Scenario: Non-empty folder deletion skipped
- **WHEN** delta sync detects that a folder was deleted on the server, but the local placeholder directory is non-empty (e.g., it contains child placeholders not yet processed)
- **THEN** the system logs a debug message and skips the folder removal (child deletions will be processed in the same or subsequent delta sync, and the folder will be removed once empty)

### Requirement: Delta sync result path resolution
The system SHALL resolve the mount-relative filesystem path for each changed or deleted item returned by delta sync. Path resolution SHALL use the `parentReference.path` field from the Microsoft Graph delta response combined with the item's `name` field. The `parentReference.path` value SHALL have the `/drive/root:` or `/drives/{drive-id}/root:` prefix stripped to produce a path relative to the mount root.

#### Scenario: Item with standard parent path
- **WHEN** a delta sync item has `parentReference.path` of `/drive/root:/Documents/Reports` and `name` of `quarterly.xlsx`
- **THEN** the system resolves the mount-relative path as `Documents/Reports/quarterly.xlsx`

#### Scenario: Item at drive root
- **WHEN** a delta sync item has `parentReference.path` of `/drive/root:` (or missing path) and `name` of `readme.txt`
- **THEN** the system resolves the mount-relative path as `readme.txt`

#### Scenario: Item with unresolvable path
- **WHEN** a delta sync item has no `parentReference` or the `parentReference.path` field is `None`
- **THEN** the system logs a warning and skips the placeholder update for that item
- **AND** the item's cache updates (memory, SQLite, disk invalidation) are unaffected — only the NTFS placeholder update is skipped

### Requirement: Platform-gated placeholder update function
The system SHALL expose a public function in `cloudmount-vfs` for applying post-delta-sync placeholder updates, gated with `#[cfg(target_os = "windows")]`. The function SHALL accept the mount path, a list of changed items with their resolved relative paths, and a list of deleted item relative paths. The function SHALL NOT require access to the `CfMountHandle` or the `SyncFilter` instance.

#### Scenario: Placeholder updates applied on Windows
- **WHEN** the app orchestration layer calls the placeholder update function after a successful delta sync on Windows
- **THEN** the function iterates over changed items, updates each placeholder's metadata and dehydrates it, then iterates over deleted items and removes each placeholder

#### Scenario: Function not compiled on non-Windows platforms
- **WHEN** the codebase is compiled for Linux or macOS
- **THEN** the placeholder update function is not included in the binary (gated by `#[cfg(target_os = "windows")]`)
