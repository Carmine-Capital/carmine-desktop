## Context

carminedesktop serves file content through a three-tier cache: memory (metadata) → SQLite (metadata) → disk (content blobs). Delta sync periodically fetches changes from the Graph API and updates the metadata tiers, but currently **does not invalidate disk content blobs** when a file's content changes on the server. Meanwhile, `open_file` serves disk-cached content unconditionally with no freshness validation.

This creates a metadata/content desynchronization: `getattr` reports the new size from fresh metadata, while `read` returns stale bytes from the disk cache. Applications like LibreOffice see a size mismatch and report corruption, or silently display stale content.

The affected code paths are:
- `crates/carminedesktop-cache/src/sync.rs` — `run_delta_sync` updates metadata but ignores disk content
- `crates/carminedesktop-vfs/src/core_ops.rs` — `open_file` trusts disk cache blindly
- `crates/carminedesktop-cache/src/disk.rs` — `DiskCache` has no concept of content version
- `crates/carminedesktop-vfs/src/fuse_fs.rs` — 60s FUSE TTL and `FUSE_WRITEBACK_CACHE` compound staleness

## Goals / Non-Goals

**Goals:**
- Ensure file content served from the mount always matches the latest known metadata
- Invalidate stale disk cache content when delta sync detects remote changes
- Add a safety-net validation in `open_file` so stale content is never served even if delta sync hasn't run
- Track content version (eTag) in the disk cache for precise staleness detection
- Bridge delta sync awareness into the read path via a dirty-inode set

**Non-Goals:**
- Real-time push notifications from the server (webhook/socket) — delta sync polling is sufficient for v1
- Changing the delta sync interval itself — that's a separate tuning concern
- Content-addressable storage or deduplication in the disk cache
- Modifying the write path or conflict detection — those already work correctly

## Decisions

### D1: Delta sync invalidates disk content on eTag mismatch

**Decision:** When `run_delta_sync` processes an upserted file item, compare the incoming eTag against the eTag stored in SQLite (pre-update). If the eTags differ, call `cache.disk.remove(drive_id, &item.id)` to delete the stale content blob.

**Rationale:** Delta sync already has the old eTag (from SQLite) and the new eTag (from the API response). Comparing them is cheap and precisely identifies content changes. Deleting the disk blob is the simplest invalidation — no need for a "revalidation" flow. The next `open_file` will cache-miss on disk and re-download.

**Alternative considered:** Mark disk entries as "needs revalidation" instead of deleting. Rejected because it adds complexity (a new state) for marginal benefit — revalidation would still require a download if the content changed, and the common case after invalidation is that the user opens the file soon after.

### D2: `open_file` validates disk cache content against metadata size

**Decision:** After loading content from the disk cache, compare `content.len()` against the metadata `DriveItem.size` from `lookup_item(ino)`. If they differ, discard the disk content and proceed to download fresh content from the Graph API.

**Rationale:** This is a safety net for the window between an online edit and the next delta sync. Size comparison catches the most damaging failure mode (the "corruption" case where the kernel tells the app a different size than what's actually available). It doesn't catch same-size-different-content, but that's addressed by D3.

**Alternative considered:** Compare eTags in `open_file` instead of size. This requires the disk cache to already store the eTag per blob (D3). Both checks are valuable — size is a fast, zero-lookup check; eTag is precise. We do both: size first (cheap), then eTag if the disk cache has one.

### D3: DiskCache stores eTag per content blob

**Decision:** Add an `etag TEXT` column to the `cache_entries` SQLite table. `DiskCache::put()` accepts an optional eTag parameter and stores it. `DiskCache::get()` returns the stored eTag alongside the content. `open_file` compares the disk cache eTag against the metadata eTag.

**Rationale:** Size comparison (D2) catches most cases but misses same-size edits. eTag is the authoritative content version identifier from the Graph API. Storing it in the existing `cache_entries` table is cheap — just one extra column and parameter.

**Alternative considered:** Store the eTag in a sidecar file next to each content blob. Rejected because we already have a SQLite tracker table — adding a column is cleaner than managing extra files.

### D4: Dirty-inode set bridges delta sync into the read path

**Decision:** Add a `DashSet<u64>` field (`dirty_inodes`) to `CoreOps`. When delta sync detects a file content change (eTag mismatch), it inserts the inode into this set. When `open_file` runs, if the inode is in the dirty set, it skips the disk cache entirely and downloads fresh content, then removes the inode from the dirty set.

**Rationale:** Even after D1 removes the disk blob, there's a race: the `open_file` call might be concurrent with delta sync, or the OS might have the old content in its kernel page cache. The dirty set is an explicit signal: "we know this file changed, don't trust any cache." It's a `DashSet` (lock-free concurrent set) for zero-contention access from both the delta sync task and FUSE threads.

**Alternative considered:** Use a `Notify` or channel per item. Rejected — too heavy for what's essentially a boolean flag per inode. `DashSet` is simple and fits the existing `DashMap` patterns in the codebase.

### D5: Reduce FUSE attribute TTL for files, keep directories longer

**Decision:** Split the FUSE `TTL` constant into two: `DIR_TTL = 30s` and `FILE_TTL = 5s`. `getattr` and `lookup` use the appropriate TTL based on whether the item is a directory or file.

**Rationale:** The current 60s TTL means the kernel may not even ask our filesystem for attributes for up to a minute after a remote change. Directories change less often and benefit from caching (avoids repeated `readdir` calls). Files change more often in the target use case (collaborative editing) and need fresher metadata. 5s is a good balance between responsiveness and avoiding excessive `getattr` round-trips.

**Alternative considered:** Use `direct_io` to bypass kernel page caching entirely. Rejected because it disables kernel read-ahead and dramatically hurts sequential read performance. The shorter TTL achieves better freshness without sacrificing throughput.

### D6: Keep FUSE_WRITEBACK_CACHE enabled

**Decision:** Keep the `FUSE_WRITEBACK_CACHE` capability. It primarily affects writes (coalescing small writes into larger ones), and the read staleness issue is already addressed by D1-D5.

**Rationale:** `FUSE_WRITEBACK_CACHE` significantly improves write performance for applications that do many small writes (like office apps saving). The read-side caching it enables is bounded by the kernel attribute TTL (now 5s for files per D5), so stale reads are limited to that window. The dirty-inode set (D4) ensures that files known to have changed remotely are always re-downloaded regardless of kernel cache state.

**Alternative considered:** Disable `FUSE_WRITEBACK_CACHE` and add `AutoInvalData`. Rejected because the write performance regression would be significant, and D5's shorter TTL already limits the kernel cache staleness window.

## Risks / Trade-offs

**[More network traffic after remote edits]** → Invalidating disk cache means re-downloading files that changed remotely. This is the correct behavior — serving stale content is worse than a re-download. For large files, the streaming download (already implemented) ensures the user doesn't wait.

**[Race between delta sync and open_file]** → A user might open a file in the brief window between a remote edit and the next delta sync. D2 (size validation) catches most cases; D4 (dirty set) catches the post-sync case. The remaining window (edit → next delta sync, same file size) is bounded by the sync interval and is a much rarer failure mode than the current "always stale" behavior.

**[DiskCache schema migration]** → Adding the `etag` column requires an `ALTER TABLE` on existing installations. SQLite `ALTER TABLE ADD COLUMN` is safe and fast. The column is nullable, so existing rows get `NULL` and are treated as "unknown eTag" (conservative — will be re-validated on next access).

**[5s file TTL increases getattr calls]** → More frequent `getattr` calls from the kernel. However, `getattr` hits the memory cache (sub-millisecond) and only goes to the network on cache miss. The memory TTL (60s) is separate from the FUSE TTL. The actual network impact is minimal.
