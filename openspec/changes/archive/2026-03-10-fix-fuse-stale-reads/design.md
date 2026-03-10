## Context

CloudMount's FUSE backend serves file reads from an OpenFileTable — a per-handle content snapshot taken at `open()` time. Meanwhile, delta sync (running every 60s in `cloudmount-cache`) detects remote changes and updates the memory cache metadata (size, eTag) and invalidates disk cache blobs. These two systems are currently disconnected: delta sync has no awareness of open file handles, and the FUSE layer has no awareness of remote changes happening mid-session.

This disconnect causes a concrete corruption scenario:
1. App opens file → handle holds 5000 bytes of content
2. Remote user edits file → server version is now 7000 bytes
3. Delta sync runs → memory cache updated to size=7000, eTag=new, disk blob removed
4. App calls `stat()` → `getattr()` returns size=7000 from memory cache
5. App calls `read()` → `read_handle()` returns 5000 bytes from the open handle
6. App sees size mismatch → corruption (LibreOffice shows error dialog, other apps may crash)

Additionally, `FUSE_WRITEBACK_CACHE` is enabled, meaning the Linux kernel caches read data in its page cache. Even if we fix the userspace side, the kernel may serve stale bytes from its own cache until the inode is invalidated.

**Crate boundary constraint**: `cloudmount-cache` (where `run_delta_sync` lives) depends on `cloudmount-core` but does NOT depend on `cloudmount-vfs` (where `OpenFileTable` lives). The notification must flow from cache → vfs without introducing a circular dependency.

## Goals / Non-Goals

**Goals:**
- Eliminate size/content mismatch for files with open FUSE handles during delta sync
- Implement close-to-open consistency: once a file handle is open, its content is stable; re-opening after close gets fresh content
- Provide a notification path from delta sync to the VFS layer for open-handle staleness
- Invalidate the kernel page cache for inodes that change remotely while open

**Non-Goals:**
- Real-time content refresh of already-open handles (NFS-like close-to-open is sufficient — content refreshes on next open, not mid-session)
- Windows CfApi changes (CfApi has its own hydration model with placeholder states)
- Changing delta sync frequency or triggering immediate sync on file access
- Supporting simultaneous read/write from multiple handles to the same inode with coherence guarantees (each handle is an independent snapshot)

## Decisions

### Decision 1: getattr returns handle size for open files

**Choice**: When `getattr()` is called for an inode that has at least one open file handle, return the size from the handle's content buffer rather than from the memory cache.

**Rationale**: This is the root cause of the corruption — `getattr()` and `read_handle()` must agree on size. The handle's content buffer is the source of truth for what `read()` will actually return. The memory cache size reflects the server state, which the handle hasn't fetched yet.

**Implementation**: Add a method `OpenFileTable::get_content_size(ino: u64) -> Option<u64>` that scans for any handle with the given inode and returns its content length. In `CoreOps::lookup_item_for_getattr()` (new method), check this first before falling through to the memory cache. `fuse_fs.rs::getattr()` calls this instead of the plain `lookup_item()`.

**Alternative considered**: Refreshing the handle's content on delta sync. Rejected because: (a) it requires downloading content synchronously during delta sync, (b) FUSE_WRITEBACK_CACHE means the kernel still has old pages, and (c) it breaks the stable-handle-content contract that applications expect.

### Decision 2: Delta sync notification via callback trait

**Choice**: Define a trait `DeltaSyncObserver` in `cloudmount-core` (shared dependency) with a method `on_inode_content_changed(ino: u64)`. `CacheManager` stores an optional `Arc<dyn DeltaSyncObserver>`. The VFS layer implements this trait to mark open handles as stale.

**Rationale**: This avoids a circular dependency between `cloudmount-cache` and `cloudmount-vfs`. The trait lives in `cloudmount-core`, which both crates already depend on. The observer pattern is simple and doesn't require async channels or additional runtime dependencies.

**Alternative considered**: `tokio::sync::broadcast` channel. Viable but heavier — requires the VFS to spawn a listener task, and the channel semantics (bounded/unbounded, lag) add complexity. The callback is simpler since delta sync already runs on a single task and the notification is synchronous (just mark a flag).

**Alternative considered**: Passing `Arc<OpenFileTable>` directly to delta sync. Rejected because it creates a direct dependency from `cloudmount-cache` on `cloudmount-vfs` types.

### Decision 3: Stale flag on OpenFile, not handle invalidation

**Choice**: Add a `stale: bool` field to `OpenFile`. When delta sync notifies that an inode changed, mark all handles for that inode as stale. The stale flag does NOT interrupt active reads — the current content continues to be served consistently. On next `open()` after the stale handle is released, the dirty-inode mechanism (already existing) ensures fresh content is downloaded.

**Rationale**: Forcibly invalidating an open handle's content mid-read would break applications worse than serving slightly old content. The key invariant is: within a single open/close session, reads are consistent. The stale flag is informational — it could be used for logging or future "notify-on-change" features.

**Alternative considered**: Immediately replacing handle content on notification. Rejected because: (a) the app may be mid-read with buffered data, (b) FUSE_WRITEBACK_CACHE means kernel pages are still old, (c) the replacement requires a Graph API download which could fail.

### Decision 4: Kernel page cache invalidation via notify_inval_inode

**Choice**: When delta sync marks an inode as changed and that inode has open handles, call `fuser::Session::notify_inval_inode()` to tell the kernel to drop its cached pages for that inode. This forces the kernel to re-issue `read()` calls to userspace on next access.

**Rationale**: With `FUSE_WRITEBACK_CACHE` enabled, the kernel aggressively caches read data. Without invalidation, even if we fix the userspace `getattr()` response, the kernel may serve stale bytes from its page cache. `notify_inval_inode(ino, offset=0, len=-1)` drops all cached pages.

**Caveat**: `notify_inval_inode()` requires a reference to the `fuser::Session`, which is only available during the mount session. The session reference must be captured during `init()` or at mount time and stored in a way accessible to the delta sync observer. If the session is unavailable (e.g., during shutdown), the invalidation is skipped (best-effort).

**Implementation**: The `DeltaSyncObserver` impl in `cloudmount-vfs` will hold an `Arc<Mutex<Option<fuser::SessionRef>>>` (or equivalent). On mount, the session is stored. On notification, the observer calls `notify_inval_inode`. This is FUSE-specific and stays entirely within `cloudmount-vfs`.

### Decision 5: getattr TTL reduction for open inodes

**Choice**: When `getattr()` returns attributes for an inode with open handles, use a TTL of 0 instead of the normal FILE_TTL (5s). This ensures the kernel re-asks userspace for attributes on every `stat()` call while the file is open, so the handle-consistent size is always used.

**Rationale**: With a 5s TTL, the kernel caches the attributes and may return stale size even after we fix `getattr()`. A zero TTL for open files ensures every `stat()` call reaches our code. This only affects files with active handles — the majority of `getattr()` calls (for browsing, etc.) still use the normal TTL.

**Alternative considered**: Always using 0 TTL. Rejected because it would massively increase FUSE overhead for directory listings and normal file browsing.

## Risks / Trade-offs

- **[Performance] getattr scans OpenFileTable per call** → The scan is O(n) where n is the number of open handles (typically < 100). If this becomes a bottleneck, add a reverse index `DashMap<u64 /* ino */, Vec<u64> /* fh */>`. Acceptable for v1.

- **[Correctness] notify_inval_inode may not be available** → The fuser session reference is only valid during the mount. If the session is dropped before the observer is notified, we skip invalidation. This is acceptable because unmount is imminent and open handles are being released anyway. Mitigation: clear the session reference on unmount.

- **[Race condition] Delta sync and open() interleave** → Between the observer marking a handle stale and the next open(), the dirty_inodes set already ensures re-download. The stale flag on the handle is defensive/informational. No new race window is introduced.

- **[FUSE_WRITEBACK_CACHE interaction] Kernel may buffer writes over stale content** → If a file is open for writing and delta sync detects a change, the kernel may have buffered writes in its page cache that it hasn't flushed yet. Calling `notify_inval_inode` forces the kernel to flush dirty pages first (per FUSE spec), then discard clean pages. This is the correct behavior — the writes will be flushed, then subsequent reads will re-fetch from userspace.

- **[Complexity] New trait + observer wiring** → Adds a small amount of abstraction. The trait has a single method and a single implementation. The wiring happens at mount setup time in `cloudmount-app`. This is a well-understood observer pattern.
