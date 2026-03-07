## Why

When a user edits a file online (e.g. in the OneDrive/SharePoint web browser), then opens the same file from the local CloudMount mount, LibreOffice (and other apps) either shows the file as corrupted or displays stale content without the online modifications. This is a data reliability issue that makes the product untrustable for daily use.

The root cause is a metadata/content desynchronization across cache tiers:

1. **Delta sync updates metadata but never invalidates stale disk content.** When `run_delta_sync` processes an upserted item (modified online), it updates the memory cache and SQLite with fresh metadata (new eTag, new size, new mtime) but leaves the old content blob on disk untouched. Only *deleted* items get their disk content removed.

2. **`open_file` serves disk-cached content without any freshness check.** When the disk cache has a blob for a file, `open_file` returns it immediately — no eTag comparison, no size comparison, nothing. Stale bytes are served as if they were current.

3. **The size mismatch between updated metadata and stale content causes corruption.** `getattr` returns the new size from fresh metadata, but `read` returns old-size bytes from the stale disk cache. Applications see a file that claims to be 80KB but only has 50KB of data — classic corruption signature.

A secondary factor is the **FUSE kernel attribute cache** (`TTL: 60s`) and the `FUSE_WRITEBACK_CACHE` capability, which add another layer of staleness on top of the userspace cache.

## What Changes

### A. Delta sync invalidates disk content for modified files

When `run_delta_sync` processes an upserted file item, compare the incoming eTag against the eTag stored in SQLite. If they differ (content changed on server), remove the disk cache entry for that item. The next `open_file` will re-download fresh content.

### B. `open_file` validates disk cache before serving

Before returning disk-cached content, compare the cached content length against the `DriveItem.size` from metadata. If they don't match, discard the stale disk entry and proceed to download fresh content. This is a safety net in case delta sync hasn't run yet or missed a change.

### C. DiskCache tracks the eTag each blob was downloaded with

Add an `etag` column to the `cache_entries` table. When storing content after download, record the eTag. This enables precise staleness detection: even if the file size happens to be the same, a different eTag means different content.

### D. Reduce FUSE attribute TTL and reconsider writeback cache

- Lower the FUSE `TTL` for regular files from 60s to something shorter (e.g. 5-10s) so that `getattr` re-checks metadata more frequently for files that may change externally. Directory TTL can stay longer.
- Evaluate whether `FUSE_WRITEBACK_CACHE` should remain enabled — it causes the kernel to coalesce and delay writes, but it also means the kernel may serve cached reads without calling our `read()`, masking freshness issues. Consider using `FUSE_AUTO_INVAL_DATA` or `direct_io` for files with known remote changes instead.

### E. Force-refresh on `open` after delta sync detects changes

When delta sync detects a file has changed (eTag mismatch), mark it in a "dirty set" (e.g. a `DashSet<u64>` of inodes). When `open_file` sees an inode in the dirty set, skip the disk cache entirely and re-download, then clear the dirty flag. This ensures the very next open after a delta sync always gets fresh content.

## Capabilities

### New Capabilities

_(none — this is a correctness fix within existing capabilities)_

### Modified Capabilities

- `cache-layer`: Delta sync now invalidates disk content for modified items. DiskCache tracks eTag per blob. A new "dirty set" mechanism bridges delta sync awareness into the read path.
- `virtual-filesystem`: `open_file` validates disk cache freshness before serving. FUSE attribute TTL may be reduced for files. Writeback cache behavior may change.

## Impact

- **Code**: `crates/cloudmount-cache/src/sync.rs` (delta sync invalidation logic), `crates/cloudmount-cache/src/disk.rs` (eTag tracking in `cache_entries`), `crates/cloudmount-vfs/src/core_ops.rs` (freshness check in `open_file`, dirty set), `crates/cloudmount-vfs/src/fuse_fs.rs` (TTL changes, writeback cache flag review).
- **Tests**: New tests for delta sync disk invalidation, stale content detection in `open_file`, eTag-based cache validation, dirty set lifecycle.
- **Dependencies**: None added.
- **Backwards compatibility**: No external API changes. Behavior is strictly improved — stale content is no longer served. Slightly more network traffic when files change remotely (re-download instead of serving stale cache), which is the correct trade-off.
- **Risk**: The eTag comparison in delta sync requires the SQLite store to already have the previous eTag for comparison. On first sync (no prior state), all items are new so there's nothing stale to invalidate. The safety net in `open_file` (size comparison) catches edge cases.
