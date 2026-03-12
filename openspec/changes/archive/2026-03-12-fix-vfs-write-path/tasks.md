## 1. Logical file size tracking (buffer truncation fix)

- [x] 1.1 Add `logical_size: Option<usize>` field to `OpenFile` struct in `core_ops.rs`
- [x] 1.2 Update `truncate()` to set `logical_size = Some(new_size)` when resizing the open file buffer smaller
- [x] 1.3 Update `write_handle()` to set `logical_size = Some(max(logical_size.unwrap_or(0), offset + data.len()))` when `logical_size` is `Some`, and use `logical_size` (not `buf.len()`) for the metadata size update
- [x] 1.4 Update `flush_handle()` to truncate the content buffer to `logical_size` (if `Some`) before writing to the writeback cache

## 2. Synchronous flush via FlushSync

- [x] 2.1 Add `FlushSync { ino: u64, done: oneshot::Sender<bool> }` variant to `SyncRequest` enum in `sync_processor.rs`
- [x] 2.2 Change `in_flight` from `HashSet<u64>` to `HashMap<u64, Vec<oneshot::Sender<bool>>>` to store oneshot senders alongside in-flight entries
- [x] 2.3 Handle `FlushSync` in `processor_loop`: if inode is already in-flight, attach the oneshot to the existing entry; otherwise bypass debounce and immediately call `spawn_upload`, storing the oneshot
- [x] 2.4 Update `handle_result()` to resolve all stored oneshot senders for the completed inode (`true` on success, `false` on failure)
- [x] 2.5 Add `wait_for_completion: bool` parameter to `flush_handle()` in `core_ops.rs` — when `true` and `sync_handle` is available, send `FlushSync` and block on the oneshot receiver with a 60-second timeout
- [x] 2.6 Update all existing callers of `flush_handle()` to pass `wait_for_completion: false` (FUSE `flush`, `release_file`, etc.)
- [x] 2.7 Update WinFsp `cleanup()` to call `flush_handle(fh, true)` (synchronous path)
- [x] 2.8 Update WinFsp `flush()` to call `flush_handle(fh, true)` (synchronous path)
- [x] 2.9 Update shutdown drain in `processor_loop` to resolve any outstanding FlushSync oneshot senders before exiting

## 3. Transient file cache cleanup

- [x] 3.1 Update `flush_inode_async()` transient file branch: after removing writeback entry, also call `cache.memory.remove(ino)` to remove the inode from memory cache
- [x] 3.2 In the same branch, resolve the parent inode from `item.parent_reference.id` via `InodeTable` and call `cache.memory.remove_child(parent_ino, &item.name)` to remove the child entry
- [x] 3.3 In the same branch, call `inodes.remove_by_item_id(&item_id)` to remove the inode mapping
- [x] 3.4 Handle the edge case where parent inode cannot be resolved: skip `remove_child`, log at debug level, still perform the other cleanup steps

## 4. Build verification

- [x] 4.1 Run `make clippy` — fix any warnings (CI enforces zero warnings)
- [x] 4.2 Run `make test` — verify no regressions in existing tests
