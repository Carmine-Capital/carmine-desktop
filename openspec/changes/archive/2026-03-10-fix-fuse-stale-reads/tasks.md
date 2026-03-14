## 1. DeltaSyncObserver trait in carminedesktop-core

- [x] 1.1 Define `DeltaSyncObserver` trait in `crates/carminedesktop-core/src/types.rs` with a single method `fn on_inode_content_changed(&self, ino: u64)`. The trait must be `Send + Sync` (for `Arc<dyn DeltaSyncObserver>`). Re-export from `crates/carminedesktop-core/src/lib.rs`.

## 2. OpenFileTable extensions in carminedesktop-vfs

- [x] 2.1 Add `stale: bool` field to `OpenFile` struct in `crates/carminedesktop-vfs/src/core_ops.rs` (line 226-230). Initialize to `false` in `OpenFileTable::insert()`.
- [x] 2.2 Add `get_content_size_by_ino(&self, ino: u64) -> Option<u64>` method to `OpenFileTable` — scans for any handle matching the inode, returns content length for `Complete` or `total_size` for `Streaming`. Returns `None` if no handle matches.
- [x] 2.3 Add `mark_stale_by_ino(&self, ino: u64)` method to `OpenFileTable` — iterates all handles, sets `stale = true` on those with matching `ino`.
- [x] 2.4 Add `has_open_handles(&self, ino: u64) -> bool` method to `OpenFileTable` — returns whether any handle exists for the given inode (used by the observer to decide whether to call `notify_inval_inode`).

## 3. Handle-consistent getattr in CoreOps

- [x] 3.1 Add `pub fn lookup_item_for_getattr(&self, ino: u64) -> Option<(DriveItem, bool)>` method to `CoreOps` in `core_ops.rs`. Returns `(item, has_open_handle)`. When an open handle exists, clones the `DriveItem` from memory cache but overrides `size` with the handle's content size from `get_content_size_by_ino`. The `bool` indicates whether the TTL should be 0.
- [x] 3.2 Update `fuse_fs.rs::getattr()` (line 198-207) to call `lookup_item_for_getattr` instead of `lookup_item`. When `has_open_handle` is true, use `Duration::ZERO` as the TTL instead of `Self::ttl_for(&item)`.
- [x] 3.3 Update `fuse_fs.rs::setattr()` (line 188) to also use `lookup_item_for_getattr` for the reply attributes, ensuring consistency after truncate.

## 4. DeltaSyncObserver implementation in carminedesktop-vfs

- [x] 4.1 Create a struct `FuseDeltaObserver` (in `core_ops.rs` or a new module) that implements `DeltaSyncObserver`. It holds a reference to `OpenFileTable` (via `Arc`) and an `Arc<Mutex<Option<...>>>` for the FUSE session reference (for `notify_inval_inode`). On `on_inode_content_changed`: call `mark_stale_by_ino`, and if `has_open_handles` and session is available, call `notify_inval_inode(ino, 0, -1)`.
- [x] 4.2 Refactor `CoreOps` to wrap `open_files: OpenFileTable` in `Arc<OpenFileTable>` so it can be shared with `FuseDeltaObserver`. Update all `self.open_files.*` call sites.
- [x] 4.3 Add a method to `CoreOps` (or `carminedesktopFs`) to produce the `Arc<dyn DeltaSyncObserver>` — constructing `FuseDeltaObserver` with the shared `Arc<OpenFileTable>`.
- [x] 4.4 Investigate `fuser::Session` API for `notify_inval_inode`. If available, store a session reference in `FuseDeltaObserver` during mount setup. If not easily available, log a TODO and skip kernel cache invalidation for now (the getattr fix alone resolves the main bug).

## 5. Wire observer into delta sync

- [x] 5.1 Add an optional `observer: Option<&dyn DeltaSyncObserver>` parameter to `run_delta_sync()` in `crates/carminedesktop-cache/src/sync.rs`. Call `observer.on_inode_content_changed(inode)` inside the `if etag_changed` block (after line 118), alongside the existing disk removal and dirty-inode marking.
- [x] 5.2 Update `DeltaSyncTimer::start()` in `sync.rs` to accept an optional `Arc<dyn DeltaSyncObserver>` and pass it through to `run_delta_sync`.
- [x] 5.3 Update `start_delta_sync()` in `crates/carminedesktop-app/src/main.rs` (line 962) to construct the `FuseDeltaObserver` from the `CoreOps`/mount state and pass it to `run_delta_sync`. This is the wiring point where cache and VFS connect.
- [x] 5.4 Update the manual `run_delta_sync` call in `commands.rs` (line 536, the "Refresh" command) to also pass the observer.
- [x] 5.5 Update the delta sync call in `main.rs` line 1360 (the setup_after_launch initial sync) to pass the observer.

## 6. Tests

- [x] 6.1 Add unit tests for `OpenFileTable::get_content_size_by_ino` in `crates/carminedesktop-vfs/tests/open_file_table_tests.rs`: test with Complete content, Streaming content, no matching handle, and multiple handles for same inode.
- [x] 6.2 Add unit tests for `OpenFileTable::mark_stale_by_ino`: test stale flag set on matching handles, not set on non-matching handles, multiple handles for same inode all marked.
- [x] 6.3 Add integration test for handle-consistent getattr: open a file, update memory cache with new size (simulating delta sync), verify `lookup_item_for_getattr` returns the handle's size not the cache's size.
- [x] 6.4 Add integration test for delta sync observer flow: set up `CacheManager` with a mock `DeltaSyncObserver`, run `run_delta_sync` with eTag change, verify `on_inode_content_changed` is called with the correct inode.
- [x] 6.5 Verify CI passes: `make clippy` and `make test` with zero warnings (RUSTFLAGS=-Dwarnings). Fix any dead code warnings from new `stale` field or unused observer methods.

## 7. Cleanup and documentation

- [x] 7.1 Add doc comments to `DeltaSyncObserver` trait, `FuseDeltaObserver` struct, `lookup_item_for_getattr`, and the new `OpenFileTable` methods.
- [x] 7.2 Add `tracing::debug!` logs at key points: observer notification, stale marking, kernel invalidation attempt, getattr returning handle size vs cache size.
