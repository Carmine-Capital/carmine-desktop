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
The system SHALL cache downloaded file content on the local disk for repeated access.

#### Scenario: File content cached after download
- **WHEN** a file is downloaded from the Graph API
- **THEN** the system stores the content in the disk cache at `<cache_dir>/<drive_id>/<item_hash>` and records the eTag for validation

#### Scenario: Serve content from disk cache
- **WHEN** a read request arrives for a file present in the disk cache with a matching eTag
- **THEN** the system serves the content from the disk cache without any API call

#### Scenario: Invalidate stale cache entry
- **WHEN** a delta sync reveals that a cached file's eTag has changed on the remote
- **THEN** the system marks the disk cache entry as stale and deletes it, causing the next read to re-download from the API

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
The system SHALL use delta queries to keep the metadata cache fresh.

#### Scenario: Periodic delta sync
- **WHEN** the configured sync interval elapses (default: 60 seconds)
- **THEN** the system performs a delta query for each mounted drive and updates all cache tiers with the changes

#### Scenario: Force refresh
- **WHEN** the user selects "Refresh" from the tray menu for a specific mount
- **THEN** the system immediately performs a delta query for that drive, regardless of the normal sync interval

### Requirement: SQLite prepared statement caching

All SQLite queries on hot paths must use cached prepared statements to avoid re-parsing SQL on every call.

#### Scenario: Repeated queries reuse prepared statements

- **WHEN** `get_item_by_id`, `get_children`, `get_delta_token`, or `upsert_item` is called multiple times
- **THEN** each call uses `conn.prepare_cached()` instead of `conn.prepare()`
- **AND** rusqlite's internal LRU cache stores the compiled statement for reuse
- **AND** no functional behavior changes — only the preparation path differs
