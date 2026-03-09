---
run: run-cloud-mount-016
work_item: fix-vfs-data-safety
intent: fix-comprehensive-review
generated: 2026-03-09
mode: validate
---

# Implementation Walkthrough: Fix VFS Data-Loss Paths

## Summary

Fixed 5 critical data-loss paths in the VFS and cache layers where user data could be silently lost during conflicts, crashes, large file operations, or new file creation. Added a shared recovery function that replaces 3 duplicated inline loops, saves unsynced local files to a recovery folder instead of discarding them, and resolves correct upload parameters from SQLite before uploading.

## Structure Overview

The fix spans three layers of the crate hierarchy. At the bottom, the writeback buffer in the cache crate now persists content to disk immediately on every write call, closing a crash window. In the middle, the VFS crate's pending module provides a shared `recover_pending_writes` function that handles both regular files (resolving parent_id from SQLite before upload) and local-only files (saving to a timestamped recovery folder with manifest). At the top, the app crate's crash recovery, headless recovery, and re-auth flush all delegate to this shared function, with the desktop variant sending a notification when files are recovered.

The streaming buffer in core_ops validates file sizes on construction, rejecting files above 256 MB (which fall through to disk-based serving) and zero-size files. Conflict detection in flush_inode now aborts the entire flush if the conflict copy upload fails, rather than silently proceeding with the main upload.

## Files Changed

### Created

None.

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-cache/src/writeback.rs` | Persist on write; sanitize `:` in filenames for Windows |
| `crates/cloudmount-vfs/src/core_ops.rs` | StreamingBuffer size cap (256 MB); conflict upload error propagation |
| `crates/cloudmount-vfs/src/pending.rs` | Shared `recover_pending_writes`; `resolve_upload_params`; `save_to_recovery` |
| `crates/cloudmount-app/src/main.rs` | Replaced 3 inline recovery loops with `recover_pending_writes` calls |
| `crates/cloudmount-app/src/notify.rs` | Added `files_recovered` notification |
| `crates/cloudmount-cache/tests/cache_tests.rs` | Added persist-on-write and colon-ID round-trip tests |
| `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | Added StreamingBuffer size cap tests |

## Key Implementation Details

### 1. WritebackBuffer Crash Safety

`write()` now calls `self.persist()` immediately after inserting into the in-memory DashMap. The persist method uses atomic write-to-temp + rename for crash safety. Each FUSE write triggers a disk write, but the OS page cache absorbs sequential writes to the same file.

### 2. StreamingBuffer Size Cap

`StreamingBuffer::new()` now returns `VfsResult<Self>` instead of `Self`. It rejects `total_size == 0` or `total_size > 256 MB`. Files exceeding 256 MB are served from disk cache instead (existing code path). This prevents OOM from `vec![0u8; total_size as usize]`.

### 3. Conflict Upload Error Propagation

In `flush_inode`, the conflict copy upload uses `if let Err(e)` instead of `let _ =`. On failure, the flush aborts with `VfsError::IoError`. The user's local copy remains in the writeback buffer for retry on next flush.

### 4. Shared Recovery Function

`pending.rs` exports `recover_pending_writes` which: lists all pending writes, saves `local:*` files to a recovery folder with manifest, uploads regular files with correct parent_id resolved from SQLite. Three call sites in main.rs now delegate to this single function.

### 5. Windows Filename Sanitization

`pending_path()` encodes `:` as `%3A` for filesystem safety. `list_pending()` reverses this. The `save_to_recovery` function uses `_` instead (one-way, for human-readable recovery filenames).

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Persist timing | Immediate on write | Debounce timer adds complexity for minimal benefit; pending dir uses OS page cache |
| StreamingBuffer cap | 256 MB | Typical OneDrive files are well under this; large files use existing disk cache path |
| Colon encoding | `%3A` percent-encoding | Unambiguous (no `%` in Graph API item IDs); reversible for list_pending |
| Recovery folder | `config_dir()/recovered/{timestamp}/` | Follows existing config convention; timestamped for multiple recovery sessions |

## Deviations from Plan

- **Added**: Windows filename sanitization for `local:*` IDs in writeback pending paths. Discovered during cross-platform code review. Without this fix, Fix 5 (persist on write) would silently fail on Windows.

## Dependencies Added

None.

## How to Verify

1. **Run tests**
   ```bash
   toolbox run -c cloudmount-build cargo test --all-targets
   ```
   Expected: All 121 tests pass (15 ignored).

2. **Lint**
   ```bash
   toolbox run -c cloudmount-build cargo clippy --all-targets --all-features
   ```
   Expected: No warnings.

3. **Manual: StreamingBuffer rejection**
   Open a file >256 MB from a mounted drive. Should see an error in logs rather than OOM.

4. **Manual: Conflict detection**
   Edit a file locally, edit the same file on OneDrive web, then save locally. Should see `.conflict.{timestamp}` file created in the same folder. If upload fails, flush should return error (visible in logs).

5. **Manual: Crash recovery**
   Create a new file, kill the process before sync. On restart, check `~/.config/cloudmount/recovered/` for saved content and manifest.

## Test Coverage

- Tests added: 5 (1 persist-on-write, 1 colon round-trip, 3 StreamingBuffer cap)
- Coverage: N/A (no cargo-tarpaulin)
- Status: All passing

## Developer Notes

- The redundant `persist()` call in `flush_inode()` (core_ops.rs) is intentional defense-in-depth. Don't remove it.
- `tokio::fs::rename` is not fully atomic on Windows with respect to power loss. Acceptable since only CloudMount accesses pending files.
- Recovery folder is never cleaned up automatically. Consider adding a cleanup mechanism in the future.
- The `resolve_upload_params` fallback (empty parent_id) preserves existing behavior for items not in SQLite. This means upload_small may fail for such items, but upload_large works by accident (uses item_id directly).

---
*Generated by FIRE Flow Run run-cloud-mount-016*
