## ADDED Requirements

### Requirement: Handle-consistent getattr for open files
The system SHALL return file size from the open file handle's content buffer when `getattr()` is called for an inode that has at least one open file handle in the OpenFileTable. This ensures that the size reported by `stat()` matches the bytes that `read()` will return, preventing size/content mismatch corruption. When no open handle exists for the inode, `getattr()` SHALL fall back to the memory cache as before.

#### Scenario: getattr returns handle size while file is open
- **WHEN** `getattr()` is called for an inode that has an open file handle with `DownloadState::Complete` content of 5000 bytes, and the memory cache reports size 7000 bytes (updated by delta sync)
- **THEN** the system returns size=5000 (from the handle's content buffer), not size=7000 (from the memory cache)

#### Scenario: getattr returns handle streaming size while download in progress
- **WHEN** `getattr()` is called for an inode that has an open file handle with `DownloadState::Streaming` and `total_size` of 10000 bytes
- **THEN** the system returns size=10000 (the expected final size from the streaming buffer's `total_size` field)

#### Scenario: getattr falls back to cache when no handle is open
- **WHEN** `getattr()` is called for an inode that has no open file handles in the OpenFileTable
- **THEN** the system returns size from the memory cache or SQLite cache, as before this change

#### Scenario: getattr uses zero TTL for open inodes
- **WHEN** `getattr()` is called for an inode that has at least one open file handle
- **THEN** the system returns a TTL of 0 seconds in the FUSE reply, ensuring the kernel re-queries attributes on every subsequent `stat()` call
- **AND** inodes without open handles continue to use the standard FILE_TTL (5 seconds)

### Requirement: Delta sync observer notification for open handles
The system SHALL define a `DeltaSyncObserver` trait in `carminedesktop-core` with a method `on_inode_content_changed(ino: u64)`. When delta sync detects an eTag change for a file inode, and a `DeltaSyncObserver` is registered on the `CacheManager`, the system SHALL call `on_inode_content_changed` for that inode. The VFS layer SHALL implement this trait to mark open handles as stale and invalidate the kernel page cache.

#### Scenario: Delta sync notifies observer on eTag change
- **WHEN** `run_delta_sync` detects that a file's eTag has changed on the server, and a `DeltaSyncObserver` is registered on `CacheManager`
- **THEN** the system calls `observer.on_inode_content_changed(inode)` for the affected inode, in addition to the existing disk cache removal and dirty-inode marking

#### Scenario: Delta sync runs without observer
- **WHEN** `run_delta_sync` detects that a file's eTag has changed, and no `DeltaSyncObserver` is registered (observer is `None`)
- **THEN** the system performs the existing behavior (disk cache removal, dirty-inode marking) without any observer notification

#### Scenario: Observer marks open handles as stale
- **WHEN** `on_inode_content_changed` is called for an inode that has one or more open handles in the OpenFileTable
- **THEN** the system sets the `stale` flag to `true` on each open handle for that inode
- **AND** active reads on those handles continue to serve the current content buffer without interruption

#### Scenario: Observer skips when no open handles exist
- **WHEN** `on_inode_content_changed` is called for an inode that has no open handles in the OpenFileTable
- **THEN** the observer takes no action (the dirty-inode set already ensures the next `open()` re-downloads)

### Requirement: Kernel page cache invalidation on remote change
The system SHALL invalidate the kernel's FUSE page cache for an inode when delta sync detects a remote content change and the inode has open file handles. The system SHALL use `fuser::Session::notify_inval_inode(ino, offset=0, len=-1)` to drop all cached pages for the inode. This is necessary because `FUSE_WRITEBACK_CACHE` causes the kernel to cache read data aggressively.

#### Scenario: Kernel cache invalidated for open inode on remote change
- **WHEN** delta sync notifies the observer that an inode's content changed, and the inode has open file handles, and the FUSE session reference is available
- **THEN** the system calls `notify_inval_inode(ino, 0, -1)` on the FUSE session to drop all kernel-cached pages for that inode
- **AND** subsequent kernel reads for that inode re-issue `read()` calls to userspace

#### Scenario: Kernel cache invalidation skipped when session unavailable
- **WHEN** delta sync notifies the observer that an inode's content changed, but the FUSE session reference is not available (e.g., during shutdown or before mount completes)
- **THEN** the system skips the `notify_inval_inode` call without error
- **AND** logs a debug-level message indicating the skip

#### Scenario: Kernel cache invalidation for inode without open handles
- **WHEN** delta sync notifies the observer that an inode's content changed, but the inode has no open file handles
- **THEN** the system does NOT call `notify_inval_inode` (the kernel will naturally re-fetch on next access since the memory cache metadata was updated)

### Requirement: Stale flag on open file handles
The system SHALL add a `stale: bool` field to the `OpenFile` struct, initialized to `false` on `open()`. The stale flag SHALL be set to `true` by the delta sync observer when a remote content change is detected for the handle's inode. The stale flag SHALL NOT interrupt active reads — the current content buffer continues to be served. The stale flag is informational for the current session; on the next `open()` after release, the existing dirty-inode mechanism ensures fresh content is downloaded.

#### Scenario: Stale flag initialized to false on open
- **WHEN** a new `OpenFile` entry is created via `OpenFileTable::insert()`
- **THEN** the `stale` field is set to `false`

#### Scenario: Stale flag set on remote change notification
- **WHEN** the delta sync observer calls `on_inode_content_changed` and an open handle exists for the inode
- **THEN** the `stale` field on the matching handle(s) is set to `true`

#### Scenario: Reads continue normally on stale handle
- **WHEN** `read_handle()` is called on a handle whose `stale` flag is `true`
- **THEN** the system serves bytes from the handle's existing content buffer, identically to a non-stale handle
- **AND** no re-download or content refresh occurs

#### Scenario: Next open after stale release re-downloads
- **WHEN** a stale handle is released and the same file is opened again
- **THEN** the system detects the dirty-inode flag (set by delta sync) and downloads fresh content from the Graph API, as per the existing dirty-inode open path
