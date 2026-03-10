## Why

The cross-platform and VFS parity audit revealed that the CfApi (Windows) backend reimplements mutation operations inline instead of delegating to `CoreOps`, bypassing conflict detection, error propagation, cache invalidation, and directory guards that FUSE relies on. Additionally, `to_string_lossy()` silently corrupts NTFS filenames containing unpaired UTF-16 surrogates, `drive_id` is passed as `account_name` in the Windows `start_mount`, and several memory-safety and resource-management patterns create OOM or data-loss risks.

## What Changes

- **CfApi `rename` delegates to `CoreOps::rename()`** instead of calling `graph.update_item()` directly. Gains eTag conflict detection, POSIX-style destination overwrite, error propagation, and parent cache invalidation.
- **CfApi `delete` delegates to `CoreOps::unlink()`/`rmdir()`** instead of calling `graph.delete_item()` directly. Gains directory-emptiness check, file/folder distinction, error propagation, and parent cache invalidation.
- **CfApi `closed()` writeback errors are propagated** — no more `let _ =` on writeback writes. On failure, logs at error level, emits a VfsEvent, and skips `flush_inode`/`mark_placeholder_synced`.
- **`to_string_lossy()` replaced with lossless OsString path handling** in `cfapi.rs::relative_path()` and `CoreOps::resolve_path()` to prevent silent corruption of NTFS filenames with unpaired surrogates.
- **`drive_id` → proper `account_name`** in Windows `start_mount` (line 909): pass the user-friendly account display name, not the Graph API drive ID.
- **`flush_inode` uses `If-Match` header** on upload to close the TOCTOU window between eTag check and upload.
- **`flush_inode` uses `Bytes::from(content)` (move)** instead of `content.clone()` to avoid triple memory copies of large files.
- **`rename` conflict copy checks upload result** before deleting the destination original — prevents silent data loss on upload failure.
- **CfApi `closed()` streams large files** to writeback instead of accumulating the entire file in memory.
- **`read_range_direct` adds a `disk.get_range()` method** to avoid loading a full file for a small range request.
- **`StreamingBuffer` uses incremental allocation** instead of `vec![0u8; total_size]` upfront.
- **`shutdown_on_signal` drains handles from mutex** before blocking unmount calls to prevent deadlock.
- **`pending.rs` recovery streams large files** via `upload_large` instead of loading them fully in memory.
- **Windows headless mode exits with a clear error** instead of silently running an idle process.
- **`start_mount` shared preamble extracted** into a helper, fixing the code duplication and the `account_name` bug in one refactor.
- **Minor cleanups**: remove dead `#[cfg(not(unix))]` block in `mount.rs`, remove stale `open` dependency in `cloudmount-auth`, fix `DEFAULT_CONFIG_TOML` Unix-only path comment, add comment on fragile `cache_dir` cfg union, make `block_on_compat` usage consistent in CfApi SyncFilter callbacks.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `virtual-filesystem`: CfApi mutation callbacks delegate to CoreOps; `to_string_lossy` replaced with lossless path handling; `flush_inode` gains `If-Match` upload and avoids content clone; writeback errors propagated; streaming writeback for large files; `read_range_direct` uses range-based disk cache read; `StreamingBuffer` uses incremental allocation; `shutdown_on_signal` releases mutex before unmount; `pending.rs` recovery streams large files; rename conflict copy checks upload result.
- `app-lifecycle`: Windows `start_mount` passes correct `account_name`; shared `start_mount` preamble extracted; headless Windows exits with clear error; minor cfg and comment cleanups.

## Impact

- **`crates/cloudmount-vfs/src/cfapi.rs`** — Major refactor of `rename`, `delete`, `closed` callbacks; path handling changed from `String` to `OsString`.
- **`crates/cloudmount-vfs/src/core_ops.rs`** — `resolve_path` signature changes from `&str` to `&OsStr`; `flush_inode` gains `If-Match` header and moves content instead of cloning; `read_range_direct` delegates to new `disk.get_range()`; `StreamingBuffer` allocation strategy changes.
- **`crates/cloudmount-vfs/src/mount.rs`** — Remove dead `#[cfg(not(unix))]` block; refactor `shutdown_on_signal` to release mutex before unmounts.
- **`crates/cloudmount-vfs/src/pending.rs`** — Large file recovery uses `upload_large` streaming.
- **`crates/cloudmount-cache/src/disk.rs`** — New `get_range()` method for partial file reads.
- **`crates/cloudmount-graph/src/lib.rs`** — Upload methods accept optional `If-Match` header.
- **`crates/cloudmount-app/src/main.rs`** — `start_mount` refactored into shared helper + platform mount; headless Windows path changed; `DEFAULT_CONFIG_TOML` comment fixed.
- **`crates/cloudmount-auth/Cargo.toml`** — Remove unused `open` dependency.
- **`crates/cloudmount-vfs/src/fuse_fs.rs`** — `resolve_path` call sites updated for `OsStr`.
