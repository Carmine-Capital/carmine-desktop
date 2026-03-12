## Why

The VFS write path has three bugs that cause data corruption, save failures on Windows, and ghost file entries. On Windows, Word/Excel saves are slow, prompt to re-save on close, then throw a permission error — because `flush_handle` returns before the upload completes and the app sees stale content on re-read verification. When a file is edited online (shorter content), opening it locally shows corrupted data (correct bytes followed by binary garbage) because `write_handle` never truncates the buffer when new content is shorter than old. On Windows, unnamed 0-byte ghost files accumulate in directory listings because transient temp files created by the safe-save pattern get their writeback entries cleaned but their memory cache entries persist.

## What Changes

- **Fix write buffer truncation**: `write_handle()` in `core_ops.rs` only grows the content buffer (`resize` if needed) but never shrinks it. When an application writes content shorter than the existing buffer (e.g., after `overwrite` or after server content changes), trailing stale bytes remain and get uploaded. Add explicit truncation: after writing at offset+len, truncate the buffer to the file's logical size if the handle was previously truncated via `set_file_size`/`overwrite`.
- **Fix flush synchronization on Windows**: `flush_handle()` sends `SyncRequest::Flush` to the SyncProcessor and returns immediately (fire-and-forget). Windows applications (Word, Excel, Notepad) verify saves by re-reading the file immediately after flush — but with 500ms debounce + upload time, they see old content, think the save failed, prompt to re-save, and then conflict with the in-flight upload. Add a synchronous completion path: when flushed from WinFsp `cleanup`/`flush` callbacks, wait for the upload to actually complete before returning.
- **Fix transient file cache orphans**: When `SyncProcessor::flush_inode_async` detects a transient file (via `is_transient_file`), it removes the writeback entry but leaves the memory cache entry intact. These orphaned `local:` entries appear as unnamed 0-byte files in directory listings until the next cache refresh. Clean up both writeback AND memory cache entries when skipping transient files.

## Capabilities

### New Capabilities

_(none — all changes are bug fixes to existing capabilities)_

### Modified Capabilities

- `virtual-filesystem`: Fix `write_handle` to truncate buffer to logical file size after writes, preventing stale trailing bytes. Add synchronous flush path that waits for upload completion instead of fire-and-forget, used by WinFsp callbacks.
- `sync-processor`: Clean up orphaned memory cache entries (and parent children maps) when skipping transient files, preventing ghost file listings.

## Impact

- **`crates/cloudmount-vfs/src/core_ops.rs`**: `write_handle()` gains truncation logic; `flush_handle()` gains optional synchronous upload path (new `SyncRequest::FlushSync` or oneshot completion channel).
- **`crates/cloudmount-vfs/src/sync_processor.rs`**: `flush_inode_async()` gains memory cache + children map cleanup for transient files; new completion-signaling mechanism for synchronous flush callers.
- **`crates/cloudmount-vfs/src/winfsp_fs.rs`**: `cleanup()`/`flush()` callbacks use the synchronous flush path.
- **`crates/cloudmount-cache/src/memory.rs`**: May need a method to remove an entry and its parent reference atomically.
- **No API changes, no new dependencies, no breaking changes.** FUSE behavior on Linux/macOS is unchanged (fire-and-forget flush is fine for POSIX apps).
