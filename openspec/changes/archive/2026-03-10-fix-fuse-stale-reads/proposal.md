## Why

When delta sync detects a remote file change while a FUSE file handle is open, the memory cache metadata (size, eTag) is updated but the OpenFileTable still holds the old content snapshot from open time. This causes `getattr()` to report the new size while `read_handle()` serves old content — a size/content mismatch that corrupts applications (e.g., LibreOffice error dialogs, truncated reads). The `FUSE_WRITEBACK_CACHE` kernel capability compounds this by caching stale reads in the kernel page cache.

## What Changes

- `getattr()` for inodes with open file handles will return the size from the handle's content buffer instead of the memory cache, preventing size/content desynchronization
- Delta sync will notify the VFS layer when open file handles become stale (eTag change detected for an inode with active handles), allowing the VFS to mark those handles and optionally invalidate the kernel's page cache via `notify_inval_inode()`
- A new notification bridge between `cloudmount-cache` (where delta sync runs) and `cloudmount-vfs` (where OpenFileTable lives) will be introduced — currently these crates have no communication path for open-handle awareness
- On next `open()` after a stale handle is released, the file will re-download fresh content (close-to-open consistency, like NFS)

## Capabilities

### New Capabilities
- `fuse-stale-read-prevention`: Close-to-open consistency for FUSE mounts — ensures getattr/read coherence for open file handles during delta sync updates, and provides a notification channel from cache to VFS for handle staleness

### Modified Capabilities
- `virtual-filesystem`: The `getattr` requirement needs a new scenario for returning handle-consistent size when a file is open, and the open file table requirement needs staleness marking behavior
- `cache-layer`: The delta sync requirement needs a new scenario for notifying open-handle staleness to the VFS layer

## Impact

- **Code**: `cloudmount-vfs` (fuse_fs.rs, core_ops.rs) and `cloudmount-cache` (sync.rs, manager.rs)
- **Cross-crate boundary**: Requires a new notification mechanism (callback or channel) from `cloudmount-cache` → `cloudmount-vfs` since delta sync and the open file table live in different crates
- **Kernel interaction**: May use `fuser::Session::notify_inval_inode()` to flush kernel page cache for changed inodes (optional enhancement, must handle FUSE session lifetime)
- **FUSE_WRITEBACK_CACHE**: Kernel writes are coalesced; invalidating the page cache is the only way to force re-reads after remote changes
- **No Windows impact**: This change is FUSE-only (Linux/macOS). CfApi has its own hydration model and is unaffected
- **No API changes**: All changes are internal; no new Graph API calls, no config changes
- **Dependencies**: No new workspace dependencies expected — the bridge can use `tokio::sync` primitives already in the workspace
