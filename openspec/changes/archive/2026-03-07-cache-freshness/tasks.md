## 1. DiskCache eTag tracking

- [x] 1.1 Add `etag TEXT` column to `cache_entries` table in `DiskCache::new()` via `ALTER TABLE ADD COLUMN` (safe migration — SQLite adds column if not exists)
- [x] 1.2 Update `DiskCache::put()` to accept an optional `etag: Option<&str>` parameter and store it in the `cache_entries` INSERT/UPSERT
- [x] 1.3 Add `DiskCache::get_with_etag()` method that returns `Option<(Vec<u8>, Option<String>)>` — content plus stored eTag
- [x] 1.4 Update all call sites of `DiskCache::put()` to pass the eTag from the `DriveItem` metadata (in `core_ops.rs` and `sync.rs`)
- [x] 1.5 Add tests: put with eTag → get_with_etag returns it; put without eTag → get_with_etag returns None; schema migration on existing DB without column

## 2. Delta sync disk cache invalidation

- [x] 2.1 In `run_delta_sync`, before applying upserts, query the existing eTag from SQLite for each upserted file item (batch query or per-item lookup)
- [x] 2.2 For each upserted file item where the old eTag differs from the new eTag, call `cache.disk.remove(drive_id, &item.id)` to delete the stale content blob
- [x] 2.3 Add tests: delta sync with eTag change removes disk cache entry; delta sync with same eTag preserves disk cache; delta sync for new item (no prior entry) does not attempt removal

## 3. Dirty-inode set

- [x] 3.1 Add `dirty_inodes: DashSet<u64>` field to `CacheManager` (shared between CoreOps and delta sync; `dashmap` already in workspace)
- [x] 3.2 Expose `mark_dirty(ino)`, `is_dirty(ino)`, `clear_dirty(ino)` convenience methods on `CoreOps` (delegate to `cache.dirty_inodes`)
- [x] 3.3 Wire delta sync to call `mark_dirty` for each inode whose eTag changed (alongside the disk removal from step 2.2)
- [x] 3.4 Add `is_dirty(ino: u64) -> bool` and `clear_dirty(ino: u64)` methods on `CoreOps`
- [x] 3.5 Add tests: mark_dirty + is_dirty returns true; clear_dirty + is_dirty returns false; concurrent access from multiple threads

## 4. `open_file` freshness validation

- [x] 4.1 In `open_file`, after checking writeback buffer and before checking disk cache, check if the inode is in the dirty set — if so, skip disk cache entirely
- [x] 4.2 Replace the `disk.get()` call with `disk.get_with_etag()`, then validate: (a) content length matches `DriveItem.size`, (b) disk eTag matches `DriveItem.etag` (if both present)
- [x] 4.3 If validation fails, call `disk.remove()` to clean up the stale blob and fall through to Graph API download
- [x] 4.4 After a fresh download (for a dirty inode or after cache miss), call `clear_dirty(ino)` and pass the eTag to `disk.put()`
- [x] 4.5 Apply the same freshness validation in `read_content()` (the non-handle-based read path used by small files)
- [x] 4.6 Add tests: open_file with stale disk cache (wrong size) triggers re-download; open_file with stale eTag triggers re-download; open_file with dirty inode skips disk cache; open_file with valid cache serves from disk

## 5. FUSE TTL split

- [x] 5.1 Replace `const TTL: Duration` with `const FILE_TTL: Duration = Duration::from_secs(5)` and `const DIR_TTL: Duration = Duration::from_secs(30)` in `fuse_fs.rs`
- [x] 5.2 Update `getattr` reply to use `FILE_TTL` for files and `DIR_TTL` for directories
- [x] 5.3 Update `lookup` reply to use `FILE_TTL` for files and `DIR_TTL` for directories
- [x] 5.4 Update `readdirplus` to use the appropriate TTL per entry type
- [x] 5.5 Update `create` and `mkdir` replies to use the appropriate TTL

## 6. Integration and verification

- [x] 6.1 Run `cargo build --all-targets` — ensure zero warnings
- [x] 6.2 Run `cargo test --all-targets` — all 126 tests pass (10 new)
- [x] 6.3 Run `cargo clippy --all-targets` — clean (--all-features skipped: desktop feature needs GTK libs)
- [x] 6.4 Run `cargo fmt --all -- --check` — formatted
