## MODIFIED Requirements

### Requirement: CfApi fetch_data immediate failure signaling
On Windows, the `fetch_data` Cloud Files API callback SHALL signal failure to the operating system immediately on any error, rather than returning without issuing any CfExecute operation. Returning without a CfExecute call leaves Windows waiting until its 60-second internal timeout expires, resulting in error 426 for the requesting process. The callback SHALL resolve the item ID from the placeholder blob set at creation time (`request.file_blob()`), without making any Graph API network call for item resolution.

All `tracing` log calls in `fetch_data` SHALL reference the absolute path variable (`abs_path`) for the file being processed. No undefined variables SHALL appear in log format strings.

#### Scenario: fetch_data — item ID decoded from placeholder blob
- **WHEN** the OS dispatches a `fetch_data` callback for a dehydrated file
- **THEN** the system decodes the item ID from `request.file_blob()` (UTF-8 bytes written at placeholder creation), looks up the corresponding inode in the inode table, and proceeds to hydrate using that inode
- **AND** no Graph API `list_children` or `get_item` call is made to resolve the file path

#### Scenario: fetch_data — blob decode or inode lookup failure
- **WHEN** the placeholder blob is missing, invalid UTF-8, or the decoded item ID has no matching inode in the inode table
- **THEN** the system returns a failure status to the OS immediately (equivalent to `CfExecute` with a non-success `CompletionStatus`)
- **AND** the OS surfaces an error to the requesting process without waiting for any timeout

#### Scenario: fetch_data — download failure
- **WHEN** the Graph API download for the required byte range fails (network error, auth error, HTTP error)
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — empty content returned
- **WHEN** the Graph API returns an empty response body for a non-zero-length file
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — path outside sync root
- **WHEN** the OS dispatches a `fetch_data` callback for a path that is not under the registered sync root
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — write_at failure mid-transfer
- **WHEN** a `write_at` call fails during the chunk transfer loop (e.g., connection closed)
- **THEN** the system aborts the transfer, logs a warning with the absolute file path and error details, and returns a failure status to the OS immediately
- **AND** Windows discards the partial transfer and leaves the file in dehydrated state

## ADDED Requirements

### Requirement: CfApi closed callback surfaces upload failures
On Windows, the `closed()` Cloud Files API callback SHALL emit a `VfsEvent::WritebackFailed` event on every error path, including when `flush_inode` fails after a successful writeback write. The system SHALL NOT silently log upload failures without notifying the user.

#### Scenario: flush_inode fails after writeback write succeeds
- **WHEN** the `closed()` callback successfully writes file content to the writeback buffer but the subsequent `flush_inode()` upload fails (network error, auth error, conflict error)
- **THEN** the system logs the error at `error` level and emits a `VfsEvent::WritebackFailed` event with the file name
- **AND** the UI surfaces a notification to the user indicating the file was not uploaded

#### Scenario: writeback write fails
- **WHEN** the `closed()` callback fails to write file content to the writeback buffer
- **THEN** the system logs the error at `error` level, emits a `VfsEvent::WritebackFailed` event, and skips the `flush_inode` call

### Requirement: CfApi state_changed invalidates parent directory cache
On Windows, when the `state_changed()` Cloud Files API callback fires for a placeholder, the system SHALL invalidate both the changed item's cache entry and its parent directory's cache entry. This ensures that subsequent `list_children` calls on the parent directory return fresh results reflecting the state change.

#### Scenario: state_changed for a file in a directory
- **WHEN** the OS fires `state_changed` for a file placeholder inside a directory
- **THEN** the system invalidates the file's inode cache entry AND the parent directory's inode cache entry
- **AND** the next `list_children` call on the parent directory fetches fresh data

#### Scenario: state_changed for the sync root itself
- **WHEN** the OS fires `state_changed` for the sync root path (empty relative components)
- **THEN** the system invalidates only the sync root's own cache entry
- **AND** no parent invalidation is attempted (the sync root has no parent within the mount)

#### Scenario: state_changed for an unresolvable path
- **WHEN** the OS fires `state_changed` for a path that cannot be resolved to an inode
- **THEN** the system skips invalidation for that path and continues processing remaining paths
