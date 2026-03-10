### Requirement: In-memory metadata cache
The system SHALL maintain an in-memory cache of file and directory metadata for fast attribute lookups and directory listings. Directory children SHALL be stored as a `HashMap<String, u64>` mapping child name to child inode, enabling O(1) lookup by name. The memory cache SHALL support surgical insertion and removal of individual children without invalidating the entire parent entry.

#### Scenario: Cached getattr response
- **WHEN** the OS requests file attributes and the metadata is in the in-memory cache with a TTL that has not expired
- **THEN** the system returns the cached metadata without making any API call, with a latency under 1 millisecond

#### Scenario: Cache miss triggers API call
- **WHEN** the OS requests metadata not present in the in-memory cache
- **THEN** the system fetches the metadata from the SQLite store (Tier 2) or the Graph API (Tier 3), populates the in-memory cache, and returns the result

#### Scenario: TTL expiration
- **WHEN** a cached metadata entry's TTL expires (default: 60 seconds, configurable)
- **THEN** the system marks the entry as stale and refreshes it on next access via the Graph API

#### Scenario: Memory pressure eviction
- **WHEN** the in-memory cache exceeds 10,000 entries
- **THEN** the system evicts the least recently used entries until the count drops below 8,000

#### Scenario: Children stored as name-to-inode map
- **WHEN** a directory's children are populated from the Graph API or SQLite
- **THEN** the system stores the children as a `HashMap<String, u64>` keyed by child filename, enabling O(1) child lookup by name

#### Scenario: Get children returns name-to-inode map
- **WHEN** `get_children` is called for a parent inode with populated children
- **THEN** the system returns `Option<HashMap<String, u64>>` containing all child name-to-inode mappings
- **AND** if the parent entry's TTL has expired, the system returns `None`

#### Scenario: Insert entry with children map
- **WHEN** `insert_with_children` is called with a parent inode, item, and `HashMap<String, u64>` children map
- **THEN** the system stores the parent entry with the children map intact
- **AND** subsequent `get_children` calls return the same map

#### Scenario: Surgical child insertion
- **WHEN** a new child is created in a cached parent directory
- **THEN** the system inserts the child's name and inode into the parent's children `HashMap` without modifying any other children or invalidating the parent entry
- **AND** the parent entry's TTL and metadata remain unchanged

#### Scenario: Surgical child removal
- **WHEN** a child is deleted from a cached parent directory
- **THEN** the system removes the child's name from the parent's children `HashMap` without modifying any other children or invalidating the parent entry
- **AND** the parent entry's TTL and metadata remain unchanged

#### Scenario: Surgical child insertion when children not populated
- **WHEN** a new child is created but the parent directory's children are not yet cached (children is `None`)
- **THEN** the surgical insertion is a no-op on the children map; the parent entry is not invalidated
- **AND** the next readdir or lookup will populate the full children map from SQLite or Graph API

### Requirement: SQLite metadata persistence
The system SHALL persist file metadata in a SQLite database that survives application restarts.

#### Scenario: Metadata stored on first sync
- **WHEN** a drive is synced for the first time via delta query
- **THEN** the system stores each item's metadata (id, name, parent_id, size, mtime, ctime, eTag, file/folder type) in the SQLite database

#### Scenario: Metadata survives restart
- **WHEN** the application restarts and a drive was previously synced
- **THEN** the system loads the metadata from the SQLite database and populates the in-memory cache, allowing immediate directory browsing without waiting for a full API sync

#### Scenario: Delta sync updates SQLite
- **WHEN** a delta query returns changed items
- **THEN** the system applies the changes (inserts, updates, deletes) to the SQLite database within a single transaction for atomicity

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

### Requirement: Cache size management
The system SHALL enforce a configurable maximum disk cache size and evict old entries when full.

#### Scenario: Cache within limit
- **WHEN** the total disk cache usage is below the configured maximum (default: 5 GB)
- **THEN** the system caches new content without eviction

#### Scenario: Cache exceeds limit
- **WHEN** caching a new file would cause the total disk cache to exceed the maximum
- **THEN** the system evicts the least recently accessed cached files until enough space is available, and then caches the new file

#### Scenario: User changes cache size
- **WHEN** the user reduces the maximum cache size in settings
- **THEN** the system immediately evicts LRU entries until the current usage is within the new limit

### Requirement: Write-back buffer
The system SHALL buffer file writes locally and upload them asynchronously. The writeback buffer SHALL serve as the persistence/crash-safety layer — it is written to on `flush`/`release`, not on every individual `write()` call.

#### Scenario: Write buffered locally
- **WHEN** a file with pending writes is flushed or released
- **THEN** the system writes the complete content from the `OpenFile` buffer to the writeback buffer and returns success to the caller without waiting for upload

#### Scenario: Buffer flushed on close
- **WHEN** a file with buffered writes is closed
- **THEN** the system initiates an asynchronous upload of the complete file to the Graph API

#### Scenario: Buffer flushed on sync
- **WHEN** the application receives an `fsync` call for a file with buffered writes
- **THEN** the system writes the `OpenFile` buffer to the writeback buffer, uploads the buffered content to the Graph API, and blocks until the upload completes

#### Scenario: Unflushed writes on crash
- **WHEN** the application terminates unexpectedly with writes in the buffer
- **THEN** on next start, the system detects pending uploads in the buffer directory and resumes uploading them

### Requirement: Delta sync integration
The system SHALL use delta queries to keep the metadata cache fresh. When delta sync detects that a file's content has changed (eTag mismatch), it SHALL invalidate the disk cache content, mark the file's inode as dirty, and notify any registered `DeltaSyncObserver` of the content change. The `CacheManager` SHALL hold an optional `Arc<dyn DeltaSyncObserver>` that can be set after construction. The `run_delta_sync` function SHALL receive the observer as a parameter and call `on_inode_content_changed(ino)` for each inode whose eTag changed, in addition to the existing disk cache removal and dirty-inode marking. The `run_delta_sync` function SHALL also return a `DeltaSyncResult` containing the list of changed items (with their `DriveItem` metadata) and the list of deleted item IDs, so that callers can propagate updates to platform-specific layers (e.g., CfApi placeholder updates on Windows).

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

### Requirement: SQLite prepared statement caching

All SQLite queries on hot paths must use cached prepared statements to avoid re-parsing SQL on every call.

#### Scenario: Repeated queries reuse prepared statements

- **WHEN** `get_item_by_id`, `get_children`, `get_delta_token`, or `upsert_item` is called multiple times
- **THEN** each call uses `conn.prepare_cached()` instead of `conn.prepare()`
- **AND** rusqlite's internal LRU cache stores the compiled statement for reuse
- **AND** no functional behavior changes — only the preparation path differs
