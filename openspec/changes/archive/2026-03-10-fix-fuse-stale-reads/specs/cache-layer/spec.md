## MODIFIED Requirements

### Requirement: Delta sync integration
The system SHALL use delta queries to keep the metadata cache fresh. When delta sync detects that a file's content has changed (eTag mismatch), it SHALL invalidate the disk cache content, mark the file's inode as dirty, and notify any registered `DeltaSyncObserver` of the content change. The `CacheManager` SHALL hold an optional `Arc<dyn DeltaSyncObserver>` that can be set after construction. The `run_delta_sync` function SHALL receive the observer as a parameter and call `on_inode_content_changed(ino)` for each inode whose eTag changed, in addition to the existing disk cache removal and dirty-inode marking.

#### Scenario: Periodic delta sync
- **WHEN** the configured sync interval elapses (default: 60 seconds)
- **THEN** the system performs a delta query for each mounted drive and updates all cache tiers with the changes

#### Scenario: Delta sync invalidates disk content for modified files
- **WHEN** a delta sync response includes a file item whose eTag differs from the eTag stored in SQLite for the same item
- **THEN** the system removes the stale disk cache content blob for that item
- **AND** the system marks the item's inode in the dirty-inode set
- **AND** if a `DeltaSyncObserver` is provided, the system calls `observer.on_inode_content_changed(inode)` for the affected inode

#### Scenario: Delta sync skips disk invalidation for metadata-only changes
- **WHEN** a delta sync response includes an item whose eTag has NOT changed (e.g., only the name or parent changed)
- **THEN** the system updates metadata in memory and SQLite but does NOT remove the disk cache content, mark the inode as dirty, or notify the observer

#### Scenario: Delta sync handles new items without prior state
- **WHEN** a delta sync response includes an item that has no prior entry in SQLite (new file)
- **THEN** the system inserts the item into all metadata caches without attempting disk cache invalidation (there is no stale blob to remove) and does NOT notify the observer (no content changed)

#### Scenario: Delta sync notifies observer for each changed inode
- **WHEN** a delta sync response includes multiple files with eTag changes, and a `DeltaSyncObserver` is provided
- **THEN** the system calls `observer.on_inode_content_changed(inode)` once for each changed inode, after marking it dirty and removing its disk cache entry

#### Scenario: Delta sync runs without observer
- **WHEN** `run_delta_sync` is called with `observer` set to `None`
- **THEN** the system performs the existing behavior (disk cache removal, dirty-inode marking) with no observer notification

#### Scenario: Force refresh
- **WHEN** the user selects "Refresh" from the tray menu for a specific mount
- **THEN** the system immediately performs a delta query for that drive, regardless of the normal sync interval
