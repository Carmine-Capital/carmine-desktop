## MODIFIED Requirements

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
