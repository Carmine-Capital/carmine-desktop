---
id: fix-vfs-data-safety
title: Fix VFS data-loss paths (conflict, flush, buffer, crash recovery)
intent: fix-comprehensive-review
complexity: high
mode: validate
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-016
completed_at: 2026-03-09T19:42:01.676Z
---

# Work Item: Fix VFS data-loss paths (conflict, flush, buffer, crash recovery)

## Description

Fix 5 CRITICAL data-loss paths in the VFS layer:

1. **Conflict upload silently ignored** (`core_ops.rs:660-666`): `let _ =` discards conflict copy upload error. If conflict upload fails, the main upload proceeds and overwrites the user's local version. Fix: propagate error, abort main upload if conflict copy fails.

2. **Unmount flush wrong params** (`pending.rs:33-38`): `flush_pending` passes `parent_id=""` and `item_id` as name. Small files may succeed via `upload_small` but large files (>=4MB) fail via `upload_large` which needs parent_id. Fix: retrieve parent_id from SQLite cache before upload.

3. **StreamingBuffer unbounded RAM** (`core_ops.rs:52`): `vec![0u8; total_size as usize]` allocates full file size. 2GB file = 2GB RAM. Negative `item.size` via `as usize` = OOM. Fix: add size cap (e.g., 256MB), validate bounds, return error for oversized.

4. **Crash recovery discards local files** (`main.rs:994-1001`): Files with `local:*` IDs removed from writeback with only a log warning. User's work vanishes silently. Fix: save content to a recovery folder (`~/.config/cloudmount/recovered/`), notify user.

5. **WritebackBuffer crash window** (`writeback.rs:30`): `write()` stores only in DashMap. Content lives in memory until explicit `persist()` call in `flush_inode`. Crash between write and flush = data loss. Fix: call `persist()` immediately in `write()`, or at minimum on a short timer.

## Acceptance Criteria

- [ ] Conflict upload failure aborts the main upload and returns error to caller
- [ ] `flush_pending` retrieves parent_id from SQLite before uploading
- [ ] StreamingBuffer rejects files above a configurable size cap with clear error
- [ ] StreamingBuffer validates `total_size` is non-negative before `as usize` cast
- [ ] Crash recovery saves `local:*` content to recovery folder instead of deleting
- [ ] User is notified (log + notification if desktop) when files are recovered
- [ ] WritebackBuffer persists to disk on write (or within a short debounce window)
- [ ] Existing tests pass, no new warnings

## Technical Notes

Key files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-vfs/src/pending.rs`, `crates/cloudmount-cache/src/writeback.rs`, `crates/cloudmount-app/src/main.rs`.

The `flush_pending` parent_id fix requires adding a SQLite query to look up the parent item ID for each pending file. The `items` table in `sqlite.rs` stores `parent_id` — use that.

For crash recovery, the recovery folder should be under `config_dir()/cloudmount/recovered/{timestamp}/`. Use `notify::` to inform the user if AppHandle is available.

## Dependencies

(none)
