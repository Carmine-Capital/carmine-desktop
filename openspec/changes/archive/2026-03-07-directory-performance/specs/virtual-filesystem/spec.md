## MODIFIED Requirements

### Requirement: Directory listing (readdir)
The system SHALL return directory contents when the operating system requests a directory listing. On Linux/macOS, the system SHALL implement both `readdir` and `readdirplus` FUSE operations. `readdirplus` SHALL return directory entries together with full file attributes in a single FUSE response, eliminating the need for per-entry `getattr` calls.

#### Scenario: List folder contents
- **WHEN** a user or application reads a mounted directory (e.g., `ls`, File Explorer browse)
- **THEN** the system returns the list of files and subdirectories with their names, sizes, types (file/folder), and modification times, sourced from the metadata cache or fetched from the Graph API on cache miss

#### Scenario: Large directory (> 1000 items)
- **WHEN** a directory contains more than 1000 items
- **THEN** the system returns all items without truncation, paginating through the Graph API as needed, and caches the complete listing

#### Scenario: readdirplus returns entries with attributes
- **WHEN** the kernel issues a `readdirplus` request for a directory
- **THEN** the system returns each child entry together with its full `FileAttr` (size, timestamps, type, permissions) and a TTL, using the same data from `CoreOps::list_children`
- **AND** the kernel caches the returned attributes, avoiding separate `getattr` calls for each entry

#### Scenario: readdirplus offset handling
- **WHEN** a `readdirplus` request includes a non-zero offset
- **THEN** the system skips entries up to that offset and returns entries starting from the offset position
- **AND** if the reply buffer fills before all entries are returned, the system stops and the kernel issues a follow-up request with the next offset

#### Scenario: readdirplus dot entries
- **WHEN** a `readdirplus` request is issued for a directory
- **THEN** the system includes `.` and `..` entries with directory type and the parent directory's attributes before the regular child entries

### Requirement: File and folder creation
The system SHALL support creating new files and folders in mounted drives. `create()` SHALL return an open file handle for the new file. After creating a child, the system SHALL surgically insert the new child into the parent's in-memory children cache rather than invalidating the entire parent entry.

#### Scenario: Create updates parent cache surgically
- **WHEN** a new file is created in a directory whose children are cached in memory
- **THEN** the system inserts the new child's name and inode into the parent's children `HashMap`
- **AND** the parent's existing children and metadata remain unchanged
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Mkdir updates parent cache surgically
- **WHEN** a new folder is created in a directory whose children are cached in memory
- **THEN** the system inserts the new folder's name and inode into the parent's children `HashMap`
- **AND** the parent's existing children and metadata remain unchanged

### Requirement: Delete operations
The system SHALL support deleting files and folders from mounted drives. After deleting a child, the system SHALL surgically remove the child from the parent's in-memory children cache rather than invalidating the entire parent entry.

#### Scenario: Unlink updates parent cache surgically
- **WHEN** a file is deleted from a directory whose children are cached in memory
- **THEN** the system removes the deleted child's name from the parent's children `HashMap`
- **AND** the parent's remaining children and metadata remain unchanged
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Rmdir updates parent cache surgically
- **WHEN** an empty folder is deleted from a directory whose children are cached in memory
- **THEN** the system removes the deleted folder's name from the parent's children `HashMap`
- **AND** the parent's remaining children and metadata remain unchanged

### Requirement: Rename and move operations
The system SHALL support renaming and moving files and folders within a mounted drive. After renaming or moving a child, the system SHALL surgically update the affected parent directories' in-memory children caches rather than invalidating them.

#### Scenario: Rename updates parent cache surgically
- **WHEN** a file or folder is renamed within the same directory
- **THEN** the system removes the old name from the parent's children `HashMap` and inserts the new name with the same inode
- **AND** no Graph API `list_children` call is triggered for the parent directory

#### Scenario: Cross-directory move updates both parents surgically
- **WHEN** a file or folder is moved from one directory to another
- **THEN** the system removes the old name from the source parent's children `HashMap` and inserts the new name into the destination parent's children `HashMap`
- **AND** no Graph API `list_children` call is triggered for either parent directory

### Requirement: O(1) child lookup by name
The system SHALL look up a child item by name under a parent directory in O(1) time using the parent's children `HashMap`, instead of iterating all children.

#### Scenario: find_child with populated cache
- **WHEN** `find_child` is called for a parent whose children are cached in memory
- **THEN** the system looks up the child name directly in the parent's `HashMap<String, u64>` and returns the matching inode and `DriveItem` without iterating other children

#### Scenario: find_child cache miss falls back to SQLite then Graph API
- **WHEN** `find_child` is called for a parent whose children are not in memory cache
- **THEN** the system falls back to SQLite, then Graph API, populating the parent's children `HashMap` on Graph API response
- **AND** the populated `HashMap` is keyed by child name for subsequent O(1) lookups
