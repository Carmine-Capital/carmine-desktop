## 1. Lossless path handling (D2)

- [x] 1.1 Change `CoreOps::resolve_path()` signature from `&str` to `&[impl AsRef<OsStr>]` (accepts pre-split components); update internal loop to compare `OsStr` against cache entries via `OsStr::to_str()` with graceful `None` on non-UTF-8
- [x] 1.2 Change `CoreOps::find_child()` to accept `&OsStr` for the name parameter; convert to `&str` for cache lookup, returning `None` if conversion fails
- [x] 1.3 Replace `cfapi.rs::relative_path()` with `relative_components()` returning `Vec<OsString>`; update all CfApi callers (`delete`, `rename`, `closed`, `dehydrate`, `state_changed`, `fetch_placeholders`) to use the new method
- [x] 1.4 Add a `resolve_parent_and_name()` helper in CfApi that splits components into `(parent_components, child_name: OsString)` for use by `delete` and `rename`
- [x] 1.5 Update FUSE `fuse_fs.rs` callers of `resolve_path` and `find_child` to pass `OsStr` (they already receive `OsStr` from the kernel — remove `.to_string_lossy()` calls)

## 2. CfApi delete delegates to CoreOps (D1)

- [x] 2.1 Refactor `cfapi.rs::delete()` to resolve parent inode via `resolve_parent_and_name()`, check `item.is_folder()`, and call `self.core.rmdir(parent_ino, &name)` or `self.core.unlink(parent_ino, &name)`
- [x] 2.2 On `CoreOps` error: log at `warn` level, do NOT call `ticket.pass()`, return `Ok(())`
- [x] 2.3 On `CoreOps` success: call `ticket.pass()` (log warning if `ticket.pass()` itself fails)
- [x] 2.4 Remove the inline `graph.delete_item()` + manual cache cleanup code from `delete()`

## 3. CfApi rename delegates to CoreOps (D1)

- [x] 3.1 Refactor `cfapi.rs::rename()` to resolve source and destination parent inodes via `resolve_parent_and_name()`, call `self.core.rename(src_parent_ino, &src_name, dst_parent_ino, &dst_name)`
- [x] 3.2 On `CoreOps` error: log at `warn` level, do NOT call `ticket.pass()`, return `Ok(())`
- [x] 3.3 On `CoreOps` success: call `ticket.pass()` (log warning if `ticket.pass()` itself fails)
- [x] 3.4 Remove the inline `graph.update_item()` + `resolve_parent_item_id()` code from `rename()`

## 4. CfApi closed writeback error propagation

- [x] 4.1 Add `VfsEvent::WritebackFailed { file_name: String }` variant to the `VfsEvent` enum in `core_ops.rs`
- [x] 4.2 In `cfapi.rs::closed()`, check the result of `writeback.write()` — on error, log at `error` level, emit `VfsEvent::WritebackFailed`, skip `flush_inode` and `mark_placeholder_synced`, return early
- [x] 4.3 In `cfapi.rs::closed()` small-file path, check `std::fs::read()` result — on error, log at `error` level, emit `VfsEvent::WritebackFailed`, return early (don't silently `return`)
- [x] 4.4 In `main.rs` event forwarding loop, handle `VfsEvent::WritebackFailed` by calling `notify::writeback_failed()`
- [x] 4.5 Add `notify::writeback_failed(app, file_name)` function in `notify.rs`

## 5. CfApi closed streaming writeback for large files (D5)

- [x] 5.1 Add `Writeback::write_chunk(drive_id, item_id, offset, chunk)` method that appends to the writeback file on disk
- [x] 5.2 Refactor `cfapi.rs::closed()` large-file path to read 64 KiB chunks from `BufReader` and call `writeback.write_chunk()` for each, instead of accumulating into `Vec`
- [x] 5.3 On any chunk write error, log at `error`, emit `VfsEvent::WritebackFailed`, return early

## 6. Conditional upload with If-Match (D3)

- [x] 6.1 Add `if_match: Option<&str>` parameter to `GraphClient::upload_small()` — sets `If-Match` header when `Some`
- [x] 6.2 Add `if_match: Option<&str>` parameter to `GraphClient::upload_large()` — sets `If-Match` header on session creation
- [x] 6.3 In `core_ops.rs::flush_inode()`, pass the server eTag from the conflict check as `if_match` to the upload call
- [x] 6.4 Handle 412 Precondition Failed in upload methods — return a new `Error::PreconditionFailed` variant
- [x] 6.5 In `flush_inode()`, treat `PreconditionFailed` as a conflict: trigger conflict copy path

## 7. flush_inode content move instead of clone (D4)

- [x] 7.1 Restructure `flush_inode()` so the conflict copy only clones `content` when a conflict is actually detected (lazy clone)
- [x] 7.2 Change the main upload path to use `Bytes::from(content)` (move) instead of `Bytes::from(content.clone())`

## 8. Rename conflict copy safety

- [x] 8.1 In `core_ops.rs::rename()`, check the result of the conflict copy `upload_small()` call
- [x] 8.2 If conflict copy upload fails, return an error instead of proceeding with destination deletion
- [x] 8.3 Log at `error` level if conflict copy upload fails

## 9. Memory-efficient content handling

- [x] 9.1 Add `DiskCache::get_range(drive_id, item_id, offset, length) -> Option<Vec<u8>>` using `seek()` + `read_exact()`
- [x] 9.2 Update `core_ops.rs::read_range_direct()` to call `disk.get_range()` instead of `disk.get()`
- [x] 9.3 Refactor `StreamingBuffer::new()` to use `BTreeMap<u64, Vec<u8>>` with 256 KiB chunks instead of `vec![0u8; total_size]`
- [x] 9.4 Update `StreamingBuffer::append_chunk()` and read methods for the chunk-based buffer
- [x] 9.5 In `pending.rs::recover_single()`, use `graph.upload_large()` for files exceeding `SMALL_FILE_LIMIT`

## 10. shutdown_on_signal mutex fix (D8)

- [x] 10.1 In `mount.rs::shutdown_on_signal()`, drain handles via `std::mem::take(&mut *mounts.lock().unwrap())` before iterating unmounts
- [x] 10.2 In `cfapi.rs::shutdown_on_signal()`, same pattern — drain handles from mutex first
- [x] 10.3 Remove dead `#[cfg(not(unix))]` block from `mount.rs::shutdown_on_signal()`

## 11. start_mount refactor and account_name fix (D9)

- [x] 11.1 Create `start_mount_common()` helper returning a `MountContext` struct (drive_id, mountpoint, cache, inodes, event_tx/rx, mount_config ref)
- [x] 11.2 Refactor Linux/macOS `start_mount` to call `start_mount_common()` then construct `MountHandle`
- [x] 11.3 Refactor Windows `start_mount` to call `start_mount_common()` then construct `CfMountHandle` with `mount_config.name` as `account_name` (not `drive_id`)
- [x] 11.4 Verify `account_name` sanitization (replace `!` with `_`) is applied to `mount_config.name`

## 12. Windows headless error exit (D10)

- [x] 12.1 In `run_headless()`, add an early `#[cfg(target_os = "windows")]` block that prints a clear error to stderr and calls `std::process::exit(1)`
- [x] 12.2 Remove the per-mount `warn!` loop for Windows in the mount iteration (now unreachable)

## 13. Minor cleanups

- [x] 13.1 Remove `open = { workspace = true }` from `crates/cloudmount-auth/Cargo.toml`
- [x] 13.2 Fix `DEFAULT_CONFIG_TOML` comment to be platform-agnostic (e.g., "Location: see --print-config-path")
- [x] 13.3 Add comment on the `cache_dir` import explaining the cfg union rationale
- [x] 13.4 Make CfApi SyncFilter callbacks use `block_on_compat()` consistently instead of mixing `rt().block_on()`

## 14. Verification

- [x] 14.1 `make clippy` passes on all targets (Linux, macOS, Windows) with zero warnings
- [x] 14.2 `make test` passes — existing tests still green
- [x] 14.3 Verify `open_file_table_tests` pass with new `StreamingBuffer` chunk-based allocation
- [x] 14.4 Verify `flush_pending_tests` pass with `If-Match` parameter additions
- [x] 14.5 Manual smoke test: mount on Linux, create/rename/delete files, verify conflict detection works
