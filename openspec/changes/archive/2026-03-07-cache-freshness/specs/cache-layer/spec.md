## MODIFIED Requirements

### Requirement: Disk content cache
The system SHALL cache downloaded file content on the local disk for repeated access. The disk cache SHALL track the eTag of each cached content blob. Content SHALL only be served from the disk cache if it passes freshness validation against the current metadata.

#### Scenario: File content cached after download
- **WHEN** a file is downloaded from the Graph API
- **THEN** the system stores the content in the disk cache at `<cache_dir>/<drive_id>/<item_hash>`, records the file size, and records the eTag from the `DriveItem` metadata at the time of download

#### Scenario: Serve content from disk cache
- **WHEN** a read request arrives for a file present in the disk cache with a matching eTag
- **THEN** the system serves the content from the disk cache without any API call

#### Scenario: Invalidate stale cache entry
- **WHEN** a delta sync reveals that a cached file's eTag has changed on the remote
- **THEN** the system deletes the stale disk cache content blob and removes the tracker entry, causing the next read to re-download from the API

#### Scenario: Disk cache eTag stored alongside content
- **WHEN** a file's content is written to the disk cache via `DiskCache::put()`
- **THEN** the system stores the provided eTag in the `cache_entries` table alongside the drive_id, item_id, and file_size
- **AND** the eTag is retrievable via `DiskCache::get_with_etag()`

#### Scenario: Disk cache content with unknown eTag
- **WHEN** a disk cache entry exists without a stored eTag (e.g., from before the schema migration)
- **THEN** the system treats the entry as potentially stale and falls through to size-based validation

#### Scenario: Schema migration adds eTag column
- **WHEN** the application starts and the `cache_entries` table does not have an `etag` column
- **THEN** the system performs `ALTER TABLE cache_entries ADD COLUMN etag TEXT` to add the column
- **AND** existing entries receive a NULL eTag value

### Requirement: Delta sync integration
The system SHALL use delta queries to keep the metadata cache fresh. When delta sync detects that a file's content has changed (eTag mismatch), it SHALL invalidate the disk cache content and mark the file's inode as dirty.

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

## ADDED Requirements

### Requirement: Dirty-inode set for cache freshness
The system SHALL maintain a set of inode numbers known to have stale or invalidated content. Delta sync populates this set when it detects content changes. The read path consults this set to skip cached content for dirty inodes.

#### Scenario: Delta sync marks inode as dirty
- **WHEN** delta sync detects a file's eTag has changed on the server
- **THEN** the system inserts the file's inode number into the dirty-inode set

#### Scenario: Open file skips disk cache for dirty inode
- **WHEN** `open_file` is called for an inode that is in the dirty-inode set
- **THEN** the system skips the disk cache lookup entirely and downloads fresh content from the Graph API
- **AND** after successful download, the system removes the inode from the dirty-inode set
- **AND** the fresh content is stored in the disk cache with the current eTag

#### Scenario: Dirty set is lock-free and concurrent
- **WHEN** delta sync marks an inode as dirty concurrently with a FUSE thread calling `open_file`
- **THEN** both operations complete without blocking each other (the set uses a concurrent data structure)

#### Scenario: Dirty set cleared on unmount
- **WHEN** a drive is unmounted
- **THEN** the dirty-inode set is cleared for that drive's inodes (or the entire set if only one drive is mounted)
