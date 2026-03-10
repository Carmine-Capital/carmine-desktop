## MODIFIED Requirements

### Requirement: Delta sync integration
The system SHALL use delta queries to keep the metadata cache fresh. When delta sync detects that a file's content has changed (eTag mismatch), it SHALL invalidate the disk cache content and mark the file's inode as dirty. The `run_delta_sync` function SHALL return a `DeltaSyncResult` containing the list of changed items (with their `DriveItem` metadata) and the list of deleted item IDs, so that callers can propagate updates to platform-specific layers (e.g., CfApi placeholder updates on Windows).

#### Scenario: Periodic delta sync
- **WHEN** the configured sync interval elapses (default: 60 seconds)
- **THEN** the system performs a delta query for each mounted drive and updates all cache tiers with the changes

#### Scenario: Delta sync invalidates disk content for modified files
- **WHEN** a delta sync response includes a file item whose eTag differs from the eTag stored in SQLite for the same item
- **THEN** the system removes the stale disk cache content blob for that item
- **AND** the system marks the item's inode in the dirty-inode set

#### Scenario: Delta sync skips disk invalidation for metadata-only changes
- **WHEN** a delta sync response includes an item whose eTag has NOT changed (e.g., only the name or parent changed)
- **THEN** the system updates metadata in memory and SQLite but does NOT remove the disk cache content or mark the inode as dirty

#### Scenario: Delta sync handles new items without prior state
- **WHEN** a delta sync response includes an item that has no prior entry in SQLite (new file)
- **THEN** the system inserts the item into all metadata caches without attempting disk cache invalidation (there is no stale blob to remove)

#### Scenario: Force refresh
- **WHEN** the user selects "Refresh" from the tray menu for a specific mount
- **THEN** the system immediately performs a delta query for that drive, regardless of the normal sync interval

#### Scenario: Delta sync returns changed items
- **WHEN** `run_delta_sync` completes successfully and one or more file items had eTag changes
- **THEN** the returned `DeltaSyncResult` SHALL contain each changed `DriveItem` in its `changed_items` field
- **AND** items whose eTag did NOT change (metadata-only updates) SHALL NOT appear in `changed_items`

#### Scenario: Delta sync returns deleted item paths
- **WHEN** `run_delta_sync` completes successfully and one or more items were detected as deleted
- **THEN** the returned `DeltaSyncResult` SHALL contain each deleted item's ID in its `deleted_ids` field
- **AND** the deleted items' `DriveItem` metadata (including `parentReference.path` and `name`) SHALL be captured BEFORE the items are removed from the caches, so that callers can resolve filesystem paths for cleanup

#### Scenario: Delta sync result is empty when no changes
- **WHEN** `run_delta_sync` completes successfully and the delta response contains no changed or deleted items
- **THEN** the returned `DeltaSyncResult` SHALL have empty `changed_items` and `deleted_ids` fields

#### Scenario: Delta sync returns deleted items with path information
- **WHEN** `run_delta_sync` processes a deleted item that has a prior entry in SQLite with known parent path and name
- **THEN** the `DeltaSyncResult` SHALL include the deleted item's name and parent path (resolved from the prior SQLite entry) so that the caller can construct the filesystem path for placeholder removal
