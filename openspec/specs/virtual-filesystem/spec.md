### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, each CfApi mount SHALL use a unique sync root ID by including an `account_name` discriminator in the sync root ID construction. The sync root ID format SHALL be `<provider>!<security-id>!<account_name>`. The `account_name` parameter SHALL be required when calling `CfMountHandle::mount()`.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows with an `account_name` identifier
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, registers a Cloud Files API sync root with a unique sync root ID derived from the provider name, user security ID, and account name, populates the directory with placeholder files, and the sync root appears as a first-class entry in File Explorer's navigation pane with cloud sync status icons

#### Scenario: Multiple concurrent Windows mounts
- **WHEN** two or more drives are mounted simultaneously on Windows, each with a distinct `account_name`
- **THEN** each mount SHALL have its own independent sync root registration, and CfApi callbacks SHALL be dispatched to the correct filter for each mount path

#### Scenario: Root resolution failure
- **WHEN** the drive root item cannot be fetched from the Graph API at mount time (network error, invalid drive_id, auth error)
- **THEN** the mount fails and returns an error; the mount point directory is not registered with FUSE/CfApi, and the error is logged and surfaced to the caller

### Requirement: Directory listing (readdir)
The system SHALL return directory contents when the operating system requests a directory listing.

#### Scenario: List folder contents
- **WHEN** a user or application reads a mounted directory (e.g., `ls`, File Explorer browse)
- **THEN** the system returns the list of files and subdirectories with their names, sizes, types (file/folder), and modification times, sourced from the metadata cache or fetched from the Graph API on cache miss

#### Scenario: Large directory (> 1000 items)
- **WHEN** a directory contains more than 1000 items
- **THEN** the system returns all items without truncation, paginating through the Graph API as needed, and caches the complete listing

### Requirement: File attribute retrieval (getattr)
The system SHALL return file attributes (size, timestamps, permissions) when requested by the operating system.

#### Scenario: Get attributes of a file
- **WHEN** the OS requests attributes for a file inode
- **THEN** the system returns: file size in bytes, last modified time (from Graph API `lastModifiedDateTime`), creation time (from `createdDateTime`), file type (regular file), and permissions (0644 for writable files, 0444 for read-only)

#### Scenario: Get attributes of a directory
- **WHEN** the OS requests attributes for a directory inode
- **THEN** the system returns: size 0, timestamps from Graph API, file type (directory), and permissions (0755)

### Requirement: File read operations
The system SHALL serve file read requests by fetching content from cache or the Graph API.

#### Scenario: Read cached file
- **WHEN** a read operation is issued for a file whose content is in the disk cache and the cache entry is still valid (matching eTag)
- **THEN** the system serves the requested bytes directly from the disk cache without any API call

#### Scenario: Read uncached file
- **WHEN** a read operation is issued for a file not in the disk cache
- **THEN** the system downloads the file content (or the requested range) from the Graph API, writes it to the disk cache, and returns the requested bytes

#### Scenario: Sequential read of large file
- **WHEN** a file larger than 64 MB is being read sequentially
- **THEN** the system uses read-ahead prefetching, downloading the next 16 MB chunk while the current chunk is being read

### Requirement: File write operations
The system SHALL buffer writes locally and flush them to the Graph API.

#### Scenario: Write to a file
- **WHEN** a write operation is issued to a mounted file
- **THEN** the system writes the data to a local write buffer in the disk cache and returns success immediately

#### Scenario: Flush on file close
- **WHEN** a file with pending writes is closed (release/flush)
- **THEN** the system uploads the complete modified file to the Graph API using the appropriate upload method (small or chunked) and updates the local metadata with the new eTag

#### Scenario: Write conflict detected
- **WHEN** uploading a modified file and the remote eTag differs from the local eTag (another user modified the file)
- **THEN** the system saves the local version as `<filename>.conflict.<timestamp>` in the same directory, downloads the remote version as the primary file, and emits a notification about the conflict

### Requirement: File and folder creation
The system SHALL support creating new files and folders in mounted drives.

#### Scenario: Create new empty file
- **WHEN** an application creates a new file (e.g., `touch`, Save As)
- **THEN** the system creates a placeholder in the local cache, assigns a temporary inode, and uploads the file to the Graph API on flush/close

#### Scenario: Create new folder
- **WHEN** an application creates a new directory (e.g., `mkdir`)
- **THEN** the system creates the folder via the Graph API immediately and returns the new inode

### Requirement: Delete operations
The system SHALL support deleting files and folders from mounted drives.

#### Scenario: Delete a file
- **WHEN** an application deletes a file (e.g., `rm`, Delete key in Explorer)
- **THEN** the system deletes the item via the Graph API, removes it from all cache tiers, and returns success

#### Scenario: Delete a non-empty folder
- **WHEN** an application attempts to delete a non-empty directory via `rmdir`
- **THEN** the system returns an ENOTEMPTY error (standard POSIX behavior)

### Requirement: Rename and move operations
The system SHALL support renaming and moving files and folders within a mounted drive.

#### Scenario: Rename a file in the same directory
- **WHEN** an application renames a file
- **THEN** the system calls the Graph API to rename the item and updates all cache entries

#### Scenario: Move a file to a different directory
- **WHEN** an application moves a file to a different directory within the same mount
- **THEN** the system calls the Graph API to move the item and updates the parent references in all cache tiers

### Requirement: Graceful unmount
The system SHALL cleanly unmount drives without data loss.

#### Scenario: User-initiated unmount
- **WHEN** the user clicks "Unmount" in the tray app
- **THEN** the system flushes all pending writes, waits for in-flight uploads to complete (with a 30-second timeout), unmounts the FUSE filesystem, and confirms unmount to the user

#### Scenario: Forced unmount on shutdown
- **WHEN** the system receives a shutdown signal (SIGTERM, system reboot)
- **THEN** the system flushes pending writes with a 10-second timeout, forcefully unmounts the FUSE filesystem, and saves any unflushed changes to a pending-uploads queue for retry on next start
