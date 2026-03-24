# VFS Parity Fix Plan

## Context

VFS parity review identified 7 issues across FUSE and WinFsp backends. This plan details fixes for each.

---

## Issue 1: FUSE `fsync` doesn't wait for completion (HIGH)

**File:** `crates/carminedesktop-vfs/src/fuse_fs.rs:474-486`

**Problem:** Both `flush` and `fsync` call `flush_handle(fh, false)`. `fsync` should block until data is persisted per POSIX semantics.

**Fix:**
```rust
fn fsync(
    &self,
    _req: &Request,
    _ino: INodeNo,
    fh: FileHandle,
    _datasync: bool,
    reply: ReplyEmpty,
) {
    // fsync should block until data is persisted (true), not fire-and-forget (false)
    match self.ops.flush_handle(fh.0, true) {
        Ok(()) => reply.ok(),
        Err(e) => reply.error(Self::vfs_err_to_errno(e)),
    }
}
```

---

## Issue 2: WinFsp `cleanup` silently swallows `unlink`/`rmdir` errors (HIGH)

**File:** `crates/carminedesktop-vfs/src/winfsp_fs.rs:759-765`

**Problem:** Delete-on-close uses `let _ =` on both `unlink` and `rmdir`, silently discarding errors.

**Fix:**
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
    }
}
```

---

## Issue 3: `setattr`/`set_basic_info` ignore mtime (MEDIUM) — **DECIDED: Option B**

**Files:** 
- `crates/carminedesktop-vfs/src/fuse_fs.rs:260-297`
- `crates/carminedesktop-vfs/src/winfsp_fs.rs:855-879`

**Problem:** Neither backend applies timestamp updates. FUSE ignores `_mtime` entirely. WinFsp is documented as no-op.

**Chosen approach:** Document that timestamps are server-authoritative (current behavior is intentional).

**Fix:** Add doc comments to both backends:
```rust
// In fuse_fs.rs setattr and winfsp_fs.rs set_basic_info:
// Timestamps are server-authoritative. Local mtime changes are ignored.
```

---

## Issue 4: Parent cache not invalidated after child mutations (MEDIUM)

**File:** `crates/carminedesktop-vfs/src/core_ops.rs`

**Problem:** `AGENTS.md` specifies: "After child mutations: invalidate parent's memory cache entry." None of the mutation functions do this.

**Functions affected:**
- `create_file` (line ~1501) - after `add_child`
- `mkdir` (line ~1588) - after `add_child`
- `unlink` (line ~1605) - after `remove_child`
- `rmdir` (line ~1650) - after `remove_child`
- `rename` (lines ~1774-1777) - after `remove_child`/`add_child`

**Fix for `create_file` (line ~1501):**
```rust
self.cache.memory.insert(inode, item.clone());
self.cache.memory.add_child(parent_ino, name, inode);
self.cache.memory.invalidate(parent_ino);  // ADD THIS
```

**Fix for `mkdir` (line ~1588):**
```rust
self.cache.memory.insert(inode, folder_item.clone());
self.cache.memory.add_child(parent_ino, name, inode);
self.cache.memory.invalidate(parent_ino);  // ADD THIS
```

**Fix for `unlink` (line ~1605):**
```rust
self.cache.memory.remove_child(parent_ino, name);
self.cache.memory.invalidate(parent_ino);  // ADD THIS
self.cleanup_deleted_item(&item_id, child_ino);
```

**Fix for `rmdir` (line ~1650):**
```rust
self.cache.memory.invalidate(child_ino);
self.cache.memory.remove_child(parent_ino, name);
self.cache.memory.invalidate(parent_ino);  // ADD THIS
```

**Fix for `rename` (lines ~1774-1777):**
```rust
self.cache.memory.remove_child(parent_ino, name);
self.cache.memory.invalidate(parent_ino);  // ADD THIS
self.cache
    .memory
    .add_child(new_parent_ino, new_name, child_ino);
if new_parent_ino != parent_ino {
    self.cache.memory.invalidate(new_parent_ino);  // ADD THIS
}
```

---

## Issue 5: WinFsp `overwrite` doesn't check dirty flag (MEDIUM)

**File:** `crates/carminedesktop-vfs/src/winfsp_fs.rs:698-718`

**Problem:** `overwrite` unconditionally truncates to 0. If file has unsaved local modifications, they are lost.

**Fix:**
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
    // Check if file has unsaved local modifications
    if self.ops.is_dirty(context.ino) {
        tracing::warn!(
            ino = context.ino,
            "overwrite on dirty file - local changes will be discarded"
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

**Problem:** `release` receives `_flush: bool` but ignores it. If OS requests flush before release, dirty data could be orphaned.

**Fix:**
```rust
fn release(
    &self,
    _req: &Request,
    _ino: INodeNo,
    fh: FileHandle,
    _flags: OpenFlags,
    _lock_owner: Option<LockOwner>,
    flush: bool,  // USE THIS
    reply: ReplyEmpty,
) {
    let result = if flush {
        self.ops.flush_handle(fh.0, true)
    } else {
        self.ops.release_file(fh.0)
    };
    
    match result {
        Ok(()) => reply.ok(),
        Err(e) => reply.error(Self::vfs_err_to_errno(e)),
    }
}
```

---

## Issue 7: WinFsp `set_delete` emptiness check vs `cleanup` path (LOW)

**File:** `crates/carminedesktop-vfs/src/winfsp_fs.rs`

**Problem:** `set_delete` checks if directory is non-empty, but `cleanup`'s delete-on-close calls `rmdir` directly without explicit emptiness check (though `rmdir` in CoreOps does check internally).

**Fix:** Already handled by CoreOps `rmdir` (line 1626). Ensure error is logged in `cleanup` (Issue 2 fix handles this).

---

## Summary of Changes

| Issue | File | Change |
|-------|------|--------|
| 1 | `fuse_fs.rs:482` | Change `false` to `true` in `fsync` |
| 2 | `winfsp_fs.rs:759-765` | Log errors instead of `let _ =` |
| 3 | Both backends | Add doc: "Timestamps are server-authoritative" |
| 4 | `core_ops.rs` | Add `invalidate(parent_ino)` after 5 mutations |
| 5 | `winfsp_fs.rs:698-718` | Check `is_dirty` before truncate |
| 6 | `fuse_fs.rs:458-472` | Handle `flush` parameter in `release` |

---

## Testing

After implementing fixes, verify:
1. `test_vfs_fsync_blocks_until_persist()` - confirm fsync with `true` waits
2. `test_vfs_cleanup_logs_errors()` - confirm errors are logged, not silently dropped
3. `test_vfs_parent_cache_invalidated()` - confirm parent cache is invalidated after create/delete/rename
4. `test_vfs_overwrite_warns_on_dirty()` - confirm warning logged when overwriting dirty file
5. `test_vfs_release_flushes_when_requested()` - confirm release with `flush=true` flushes data

---

## Decisions Made

- **Issue 3 (timestamps):** Option B — document that timestamps are server-authoritative
