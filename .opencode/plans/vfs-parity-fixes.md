# VFS Parity Fix Plan

## Context

VFS parity review identified 7 issues across FUSE and WinFsp backends.
Post-verification against the codebase, 6 are real fixes and 1 is dropped.

---

## Issue 1: FUSE `fsync` doesn't wait for completion (HIGH)

**File:** `crates/carminedesktop-vfs/src/fuse_fs.rs:474-486`

**Problem:** Both `flush` and `fsync` call `flush_handle(fh, false)`. `fsync` should block until data is persisted per POSIX semantics. `flush_handle(_, true)` sends a `FlushSync` request and blocks on a oneshot channel until upload completes (60 s timeout).

**Fix:** One-character change — `false` to `true` on line 482:
```rust
fn fsync(
    &self,
    _req: &Request,
    _ino: INodeNo,
    fh: FileHandle,
    _datasync: bool,
    reply: ReplyEmpty,
) {
    match self.ops.flush_handle(fh.0, true) {
        Ok(()) => reply.ok(),
        Err(e) => reply.error(Self::vfs_err_to_errno(e)),
    }
}
```

**Note:** FUSE `flush` (called on `close(2)`) correctly uses `false` — POSIX close does not require durability. Only `fsync` needs `true`.

---

## Issue 2: WinFsp `cleanup` silently swallows `unlink`/`rmdir` errors (HIGH)

**File:** `crates/carminedesktop-vfs/src/winfsp_fs.rs:759-764`

**Problem:** Delete-on-close uses `let _ =` on both `unlink` and `rmdir`, silently discarding errors. Since `cleanup` returns `()` per the WinFsp trait, errors cannot propagate to the OS.

**Fix:** Log the error and emit a `VfsEvent` so the user sees feedback via the notification system (matching the `UploadFailed` pattern at lines 728-742). Requires adding a `DeleteFailed` variant to `VfsEvent` in `core_ops.rs`.

**Step 1 — Add variant to `VfsEvent` (`core_ops.rs:346-358`):**
```rust
pub enum VfsEvent {
    // ... existing variants ...
    /// A delete-on-close operation failed during cleanup.
    DeleteFailed { file_name: String, reason: String },
}
```

**Step 2 — Replace `let _ =` block (`winfsp_fs.rs:759-764`):**
```rust
if let Some(parent_ino) = parent_ino {
    let result = if context.is_dir {
        self.ops.rmdir(parent_ino, name)
    } else {
        self.ops.unlink(parent_ino, name)
    };
    if let Err(e) = result {
        tracing::warn!(
            parent_ino,
            name = %name,
            is_dir = context.is_dir,
            "cleanup delete-on-close failed: {e}"
        );
        self.ops.send_event(VfsEvent::DeleteFailed {
            file_name: name.to_string(),
            reason: format!("{e:?}"),
        });
    }
}
```

**Step 3 — Handle the new variant** in `carminedesktop-app` notification dispatch (wherever `VfsEvent` is matched).

---

## Issue 3: `setattr`/`set_basic_info` ignore mtime (MEDIUM) — **DECIDED: Option B**

**Files:**
- `crates/carminedesktop-vfs/src/fuse_fs.rs:260-297`
- `crates/carminedesktop-vfs/src/winfsp_fs.rs:855-879`

**Problem:** Neither backend applies timestamp updates. FUSE ignores all 6 timestamp params (`_atime`, `_mtime`, `_ctime`, `_crtime`, `_chgtime`, `_bkuptime`). WinFsp `set_basic_info` is a documented no-op.

**Chosen approach:** Document that timestamps are server-authoritative (current behavior is intentional).

**Fix — `fuse_fs.rs` `setattr` (add above the `if let Some(new_size)` block):**
```rust
// Timestamps are server-authoritative — local mtime/atime changes are
// intentionally ignored. With FUSE_WRITEBACK_CACHE enabled, the kernel
// sends setattr with updated mtime after writes; the divergence between
// kernel-cached mtime and server mtime resolves on the next delta sync.
// Only size (truncation) is handled here.
```

**Fix — `winfsp_fs.rs` `set_basic_info` (expand existing comment on line 865):**
```rust
// Timestamps are server-authoritative — local timestamp changes
// (creation, last-access, last-write, change) are intentionally
// ignored. The server sets authoritative timestamps on upload.
// Return current FileInfo unchanged.
```

---

## Issue 4: ~~Parent cache not invalidated after child mutations~~ — **DROPPED**

**Original claim:** `add_child`/`remove_child` don't invalidate the parent, so an explicit `invalidate(parent_ino)` is needed after every child mutation.

**Verification result:** This is wrong. The memory cache in `carminedesktop-cache/src/memory.rs` works as follows:
- `add_child(parent, name, child)` — surgically inserts into the parent's `children` HashMap in-place. Children map stays consistent.
- `remove_child(parent, name)` — surgically removes from the HashMap. Children map stays consistent.
- `invalidate(ino)` — calls `self.entries.remove(&ino)`, **destroying the entire cache entry** (DriveItem metadata + children map).

Calling `invalidate(parent_ino)` after every mutation would:
1. Destroy valid cached children maps, forcing a full Graph API re-fetch on next `readdir`/`find_child`
2. Create race conditions — concurrent sibling operations would miss the cache between invalidation and re-fetch
3. Degrade offline mode — the parent could vanish from cache until re-fetched

The existing `add_child`/`remove_child` calls are the correct approach. The `AGENTS.md` convention that led to this issue has been corrected (see below).

**Action:** No code change. Fix `crates/carminedesktop-vfs/AGENTS.md` to correct the misleading convention.

---

## Issue 5: WinFsp `overwrite` doesn't flush dirty data (MEDIUM)

**File:** `crates/carminedesktop-vfs/src/winfsp_fs.rs:698-718`

**Problem:** `overwrite` (triggered by `CREATE_ALWAYS`/`TRUNCATE_EXISTING`) unconditionally truncates to 0. If the file has dirty content from a concurrent writer that hasn't been uploaded yet, that data is silently lost.

**Fix:** Best-effort flush before truncating. The user intends to replace the file, so we proceed with truncation regardless — but we give the pending upload a chance to complete first. `CoreOps::is_dirty(ino)` exists at `core_ops.rs:660`.

```rust
fn overwrite(
    &self,
    context: &Self::FileContext,
    _file_attributes: u32,
    _replace_file_attributes: bool,
    _allocation_size: u64,
    _extra_buffer: Option<&[u8]>,
    file_info: &mut FileInfo,
) -> winfsp::Result<()> {
    // Best-effort: flush pending dirty data before truncating.
    // The user intends to replace the file, so we proceed regardless.
    if self.ops.is_dirty(context.ino)
        && let Some(fh) = context.fh
        && let Err(e) = self.ops.flush_handle(fh, true)
    {
        tracing::warn!(
            ino = context.ino,
            "overwrite: best-effort flush of dirty data failed: {e}"
        );
    }

    self.ops
        .truncate(context.ino, 0)
        .map_err(vfs_err_to_ntstatus)?;

    let item = self
        .ops
        .lookup_item(context.ino)
        .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

    *file_info = item_to_file_info(&item, Some(0));
    Ok(())
}
```

---

## Issue 6: FUSE `release` ignores `_flush` parameter (MEDIUM)

**File:** `crates/carminedesktop-vfs/src/fuse_fs.rs:458-472`

**Problem:** `release` receives `_flush: bool` but ignores it. When the kernel sets `flush=true`, dirty data should be flushed before the handle is released.

**Fix:** Non-blocking best-effort flush (`false`, not `true`) before `release_file`. Using `true` would block the FUSE thread up to 60 s, which is too aggressive for release. `release_file` already writes dirty content to the writeback buffer as a safety net, so errors from the flush are swallowed.

Both paths must end with `release_file` to clean up the handle from the open-file table.

```rust
fn release(
    &self,
    _req: &Request,
    _ino: INodeNo,
    fh: FileHandle,
    _flags: OpenFlags,
    _lock_owner: Option<LockOwner>,
    flush: bool,
    reply: ReplyEmpty,
) {
    // Best-effort non-blocking flush when the kernel requests it.
    // Errors are swallowed — release_file writes dirty content to
    // writeback as a safety net, and the sync processor picks it up.
    if flush {
        let _ = self.ops.flush_handle(fh.0, false);
    }
    match self.ops.release_file(fh.0) {
        Ok(()) => reply.ok(),
        Err(e) => reply.error(Self::vfs_err_to_errno(e)),
    }
}
```

---

## Issue 7: WinFsp `set_delete` emptiness check vs `cleanup` path (LOW) — **Covered by Issue 2**

**Problem:** `set_delete` checks if a directory is non-empty before allowing delete, but `cleanup`'s delete-on-close calls `rmdir` directly.

**Status:** Non-issue. `CoreOps::rmdir` (`core_ops.rs:1610-1655`) performs its own server-side emptiness check via `graph.list_children()`. The double-check is beneficial — `set_delete` gives an early `STATUS_DIRECTORY_NOT_EMPTY` error, while `CoreOps::rmdir` catches race conditions. With Issue 2's fix, `rmdir` errors in `cleanup` are now logged and surfaced to the user instead of silently swallowed.

**Action:** No additional code change needed.

---

## Summary of Changes

| Issue | Sev | File | Change |
|-------|-----|------|--------|
| 1 | HIGH | `fuse_fs.rs:482` | `false` to `true` in `fsync` |
| 2 | HIGH | `core_ops.rs` + `winfsp_fs.rs:759-764` | Add `VfsEvent::DeleteFailed`, log + emit in cleanup |
| 3 | MED | Both backends | Doc comments: timestamps are server-authoritative |
| 4 | — | **DROPPED** | Non-issue; fix AGENTS.md instead |
| 5 | MED | `winfsp_fs.rs:698-718` | Best-effort flush before truncate |
| 6 | MED | `fuse_fs.rs:458-472` | Non-blocking flush when `flush=true`, then `release_file` |
| 7 | LOW | No change | Covered by Issue 2 |

---

## Testing

After implementing fixes, verify:
1. `test_vfs_fsync_blocks_until_persist()` — confirm fsync with `true` waits
2. `test_vfs_cleanup_logs_delete_errors()` — confirm errors are logged and `DeleteFailed` event emitted
3. `test_vfs_overwrite_flushes_dirty()` — confirm best-effort flush before truncate
4. `test_vfs_release_flushes_when_requested()` — confirm release with `flush=true` triggers non-blocking flush before release_file

---

## Decisions Made

- **Issue 3 (timestamps):** Option B — document as server-authoritative
- **Issue 4 (parent cache):** Dropped — `add_child`/`remove_child` are surgical and correct; `invalidate` would be destructive. AGENTS.md convention corrected.
- **Issue 5 (overwrite):** Flush-then-truncate, not warn-then-truncate
- **Issue 6 (release flush):** Non-blocking (`false`), not blocking (`true`); always call `release_file` after
