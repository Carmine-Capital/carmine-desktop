## ADDED Requirements

### Requirement: Atomic inode allocation
The InodeTable SHALL guarantee a 1:1 mapping between `item_id` and `inode` under concurrent access. The `allocate()` method SHALL use a single lock to perform the lookup-or-insert operation atomically — no window SHALL exist between checking for an existing mapping and inserting a new one. All mutating methods (`allocate`, `reassign`, `set_root`, `remove_by_item_id`) SHALL hold a single unified lock covering both the inode-to-item and item-to-inode maps.

#### Scenario: Concurrent allocation for the same item_id
- **WHEN** two threads call `allocate("item123")` simultaneously and no mapping exists yet
- **THEN** exactly one inode SHALL be allocated, and both calls SHALL return the same inode number

#### Scenario: No ghost inode entries after concurrent access
- **WHEN** `allocate()` is called concurrently for the same `item_id`
- **THEN** the `inode_to_item` and `item_to_inode` maps SHALL contain exactly one entry each for that `item_id`, with no orphaned inode numbers

### Requirement: CfApi closed callback skips unmodified files
On Windows, the CfApi `closed()` callback SHALL skip the writeback and upload cycle when the file was not modified since last sync. The system SHALL compare the file's Last Write Time (from filesystem metadata) against the cached `DriveItem.last_modified` timestamp. If the timestamps match within a 1-second tolerance, the system SHALL return immediately without reading file content, writing to the writeback buffer, or calling `flush_inode`.

#### Scenario: Read-only file open on Windows
- **WHEN** a user opens a hydrated file in a read-only application (e.g., preview, viewer) and closes it without modification
- **THEN** the `closed()` callback SHALL detect that the file's Last Write Time matches the cached server timestamp and SHALL NOT trigger any Graph API calls

#### Scenario: Modified file close on Windows
- **WHEN** a user edits a file and saves changes, causing Windows to update the file's Last Write Time
- **THEN** the `closed()` callback SHALL detect the mtime mismatch and proceed with the full writeback and upload cycle including conflict detection

#### Scenario: Newly hydrated file close
- **WHEN** `fetch_data` hydrates a placeholder file and the user closes it without editing
- **THEN** the `closed()` callback SHALL detect that the file's Last Write Time (set by `mark_placeholder_synced`) matches the cached server timestamp and SHALL skip the upload

## MODIFIED Requirements

### Requirement: Resilient CfApi callback error handling
Each CfApi callback (`fetch_data`, `fetch_placeholders`, `delete`, `rename`, `closed`, `validate_data`, `state_changed`) SHALL handle errors gracefully without panicking or propagating unhandled exceptions across the FFI boundary. On error, each callback SHALL log sufficient context (callback name, file path, error details) and return `Ok(())` or skip the failing operation rather than returning an error that could trigger Windows error dialogs or cloud-filter panics.

Writeback failures in `closed()` (file read, writeback write, chunk write, finalize, and flush_inode) SHALL emit a `VfsEvent::WritebackFailed { file_name }` event so the UI can notify the user that their changes may not have been saved. This ensures every error path in `closed()` surfaces user feedback.

The CfApi `closed()` callback SHALL only proceed with the writeback cycle when the file has been modified since last sync (see: CfApi closed callback skips unmodified files).

The `CfMountHandle` struct SHALL name the `Connection` field without a leading underscore (i.e., `connection`, not `_connection`) because its drop order relative to `sync_root_id` is safety-critical. The field SHALL be documented to explain that it must be dropped before `sync_root_id` is unregistered.

#### Scenario: fetch_data logs path on write_at failure
- **WHEN** `ticket.write_at()` fails during chunked data transfer in `fetch_data`
- **THEN** the callback logs a warning with the file's absolute path (not an undefined variable) and breaks the write loop without panicking

#### Scenario: closed skips unmodified files
- **WHEN** `closed()` fires for a file whose Last Write Time matches the cached server timestamp
- **THEN** the callback returns immediately without reading file content or calling flush_inode

#### Scenario: closed flush_inode failure emits event
- **WHEN** `flush_inode()` returns an error after a successful writeback write in `closed()`
- **THEN** a `VfsEvent::WritebackFailed { file_name }` event is emitted and the error is logged

#### Scenario: CfMountHandle drop order correctness
- **WHEN** a `CfMountHandle` is dropped (either via `unmount()` or implicit drop)
- **THEN** the `connection` field is dropped before `sync_root_id` is unregistered, preventing Windows from rejecting the unregistration due to an active connection
