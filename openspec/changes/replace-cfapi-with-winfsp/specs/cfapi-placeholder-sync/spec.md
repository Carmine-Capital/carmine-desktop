## REMOVED Requirements

### Requirement: Placeholder metadata update after delta sync
**Reason**: WinFsp has no NTFS placeholders. There is no dehydration/rehydration model. When delta sync detects remote changes, it updates the memory and SQLite caches and marks the inode as dirty. The next `read()` or `get_file_info()` call on the WinFsp backend serves fresh data from the updated cache or re-downloads content via the dirty-inode check in CoreOps.
**Migration**: Delta sync continues to update caches and call `DeltaSyncObserver::on_inode_content_changed()`. The `WinFspDeltaObserver` marks open handles stale. No placeholder updates are needed.

### Requirement: Placeholder deletion after delta sync
**Reason**: WinFsp has no placeholder files on disk to delete. Directory listings are served dynamically from the cache via `read_directory`. When delta sync removes items from the cache, they stop appearing in directory listings immediately.
**Migration**: Delta sync cache removal is sufficient. No filesystem-level file deletion needed.

### Requirement: Delta sync result path resolution
**Reason**: Path resolution for delta sync results (`resolve_relative_path`, `resolve_deleted_path`) was only needed to locate NTFS placeholder files on disk for update/deletion. With WinFsp, delta sync only needs to update cache entries (by item ID and inode), not locate filesystem paths.
**Migration**: Delta sync updates cache entries by item ID. The `apply_delta_placeholder_updates` function and its path resolution helpers are removed.

### Requirement: Platform-gated placeholder update function
**Reason**: The `apply_delta_placeholder_updates` public function in `cloudmount-vfs` is removed along with all CfApi code. WinFsp delta sync is handled entirely through cache updates and the `DeltaSyncObserver` trait.
**Migration**: Remove the `#[cfg(target_os = "windows")]` block in `main.rs` `start_delta_sync()` that calls `apply_delta_placeholder_updates`. The delta sync loop becomes platform-uniform.

### Requirement: Mount drive as native filesystem
**Reason**: The CfApi-specific additions to this requirement (watcher thread, timer thread, writeback staging from disk, rename callback acknowledgement) are removed. The base requirement is modified in the `virtual-filesystem` delta spec to reference WinFsp instead of CfApi.
**Migration**: See `virtual-filesystem` delta spec for the updated "Mount drive as native filesystem" requirement.
