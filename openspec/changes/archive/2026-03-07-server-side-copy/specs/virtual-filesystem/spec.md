## ADDED Requirements

### Requirement: Server-side copy via copy_file_range
The system SHALL implement the FUSE `copy_file_range` operation to optimize file copies within the mount. When both source and destination are remote items and the copy covers the full file, the system SHALL use the Graph API server-side copy instead of transferring data through the client. When server-side copy is not eligible, the system SHALL fall back to an in-memory buffer copy between the open file handles.

#### Scenario: Full-file copy between two remote files
- **WHEN** `copy_file_range` is called with `offset_in == 0`, `len >= source file size`, and both the source item ID and destination parent are remote (not `local:` prefixed)
- **THEN** the system calls the Graph API copy endpoint, polls for completion, retrieves the new item metadata, reassigns the destination inode from its temporary `local:` ID to the real server item ID, updates all caches with the new item metadata, and returns the number of bytes copied (equal to the source file size)

#### Scenario: Partial range copy
- **WHEN** `copy_file_range` is called with `offset_in > 0` or `len < source file size`
- **THEN** the system falls back to reading the requested byte range from the source file handle's in-memory buffer and writing it into the destination file handle's buffer at `offset_out`, marking the destination handle as dirty

#### Scenario: Copy from a local (not yet uploaded) file
- **WHEN** `copy_file_range` is called and the source item ID starts with `local:`
- **THEN** the system falls back to the in-memory buffer copy between file handles

#### Scenario: Buffer-level fallback copies data in-memory
- **WHEN** server-side copy is not eligible and the system falls back to buffer-level copy
- **THEN** the system reads from the source handle's `OpenFile` content buffer and writes into the destination handle's `OpenFile` content buffer without any network I/O, and returns the number of bytes copied

#### Scenario: Server-side copy updates destination inode mapping
- **WHEN** a server-side copy completes successfully
- **THEN** the system calls `InodeTable::reassign()` to update the destination inode from its temporary `local:` ID to the server-assigned item ID, inserts the new `DriveItem` into the memory cache, and removes any writeback buffer entry for the old temporary ID

#### Scenario: Server-side copy failure returns error
- **WHEN** the Graph API copy operation fails (HTTP error, server-side failure, or timeout)
- **THEN** the system returns `EIO` to the FUSE caller and logs the error details

#### Scenario: Destination file handle updated after server-side copy
- **WHEN** a server-side copy completes and the destination file handle is still open
- **THEN** the system updates the open file handle's inode metadata to reflect the copied file's size and marks the handle as non-dirty (the server already has the complete data)

#### Scenario: Platform without copy_file_range support
- **WHEN** a file copy is performed on macOS (which lacks FUSE `copy_file_range`) or on Windows (CfApi)
- **THEN** the copy proceeds via the existing read+write path with no behavior change
