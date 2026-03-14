## Why

Every FUSE `read()` call reloads the entire file content from cache or network, and every `write()` call clones the full buffer to splice in new data. For a 100MB file, this means ~80GB of redundant I/O for reads and ~10GB of memcpy for writes. The current `open()` returns a dummy `FileHandle(0)` and does nothing — there is no per-handle state. This is the single highest-impact performance fix available.

## What Changes

- Introduce an `OpenFileTable` in `CoreOps` that tracks per-file-handle state (`OpenFile` entries with cached content, dirty flag, and access mode).
- `open()` loads file content once into an `OpenFile` buffer and returns a unique file handle.
- `read()` slices directly from the cached `OpenFile` buffer — zero redundant loads.
- `write()` mutates the `OpenFile` buffer in-place — O(1) per call instead of O(n) clone+splice.
- `flush()`/`fsync()` uploads only if the buffer is dirty.
- `release()` drops the buffer and decrements any refcount.
- `create()` returns a file handle tied to the new file's `OpenFile` entry.
- `truncate()` (via `setattr`) operates on the `OpenFile` buffer when the file is open.
- The writeback buffer (`WriteBackBuffer`) is no longer written to on every `write()` call — only on `flush`/`release`.

## Capabilities

### New Capabilities

_(none — this is an internal implementation change within existing capabilities)_

### Modified Capabilities

- `virtual-filesystem`: The VFS now manages per-handle open file state. `open()` returns real file handles backed by content buffers. `read`/`write`/`flush`/`release` semantics change from stateless inode-based to stateful handle-based operations. `create()` returns an open file handle.
- `cache-layer`: Write-back buffer interaction changes — writes accumulate in `OpenFile` buffers and are flushed to writeback only on `flush`/`release`, not on every `write()` call.

## Impact

- **Code**: `crates/carminedesktop-vfs/src/core_ops.rs` (new `OpenFileTable` + reworked read/write/flush/open/release/create), `crates/carminedesktop-vfs/src/fuse_fs.rs` (wire real file handles, implement `release`), `crates/carminedesktop-vfs/src/cfapi.rs` (adapt hydration to use `OpenFile` buffers).
- **Tests**: `crates/carminedesktop-app/tests/integration_tests.rs` updated for handle-based semantics.
- **Dependencies**: None added. Uses existing `DashMap` and `std::sync::atomic`.
- **Backwards compatibility**: Internal change only — no API or config changes. FUSE/CfApi external behavior unchanged.
