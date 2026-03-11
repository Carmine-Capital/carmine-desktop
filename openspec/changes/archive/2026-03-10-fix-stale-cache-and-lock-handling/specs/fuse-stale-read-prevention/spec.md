## MODIFIED Requirements

### Requirement: Kernel page cache invalidation on remote change
The system SHALL invalidate the kernel's FUSE page cache for an inode when delta sync detects a remote content change, regardless of whether the inode has open file handles. The system SHALL use `fuser::Session::notify_inval_inode(ino, offset=0, len=-1)` to drop all cached pages for the inode. This is necessary because `FUSE_WRITEBACK_CACHE` causes the kernel to cache `i_size` and read data aggressively, and only `inval_inode` forces the kernel to discard its cached values.

Additionally, `open_file` SHALL refresh metadata from the server via `get_item()` BEFORE validating the disk cache. If the server returns a different eTag than the cached metadata, the system SHALL update memory and SQLite caches, invalidate the kernel inode, and evict the stale disk cache entry before proceeding to download. If the `get_item()` call fails (network error), the system SHALL fall back to the existing disk cache validation using cached metadata.

#### Scenario: Kernel cache invalidated for open inode on remote change
- **WHEN** delta sync notifies the observer that an inode's content changed, and the inode has open file handles, and the FUSE session reference is available
- **THEN** the system calls `notify_inval_inode(ino, 0, -1)` on the FUSE session to drop all kernel-cached pages for that inode
- **AND** subsequent kernel reads for that inode re-issue `read()` calls to userspace

#### Scenario: Kernel cache invalidation skipped when session unavailable
- **WHEN** delta sync notifies the observer that an inode's content changed, but the FUSE session reference is not available (e.g., during shutdown or before mount completes)
- **THEN** the system skips the `notify_inval_inode` call without error
- **AND** logs a debug-level message indicating the skip

#### Scenario: Kernel cache invalidation for inode without open handles
- **WHEN** delta sync notifies the observer that an inode's content changed, and the inode has no open file handles
- **THEN** the system calls `notify_inval_inode(ino, 0, -1)` to force the kernel to discard its cached `i_size` for the inode

#### Scenario: Server metadata refresh before disk cache validation
- **WHEN** `open_file` is called for a non-local, non-writeback file
- **THEN** the system calls `get_item()` on the Graph API before checking the disk cache
- **AND** if the server eTag differs from the cached eTag, updates memory cache, SQLite, invalidates the kernel inode, and evicts the disk cache entry
- **AND** the subsequent disk cache check uses the fresh metadata for validation

#### Scenario: Server metadata refresh fails with network error
- **WHEN** `open_file` calls `get_item()` and the call fails due to a network error
- **THEN** the system falls back to the existing disk cache validation using cached metadata from memory/SQLite
- **AND** logs a warning about the failed refresh

#### Scenario: Stale disk cache detected after server refresh
- **WHEN** `open_file` refreshes metadata and finds the server eTag differs from the disk cache eTag
- **THEN** the disk cache entry is evicted
- **AND** the system downloads fresh content from the Graph API
