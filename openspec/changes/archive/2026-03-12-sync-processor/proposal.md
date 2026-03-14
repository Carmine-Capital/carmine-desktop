## Why

`flush_handle()` blocks the calling VFS thread (FUSE or WinFsp) during the entire upload pipeline: conflict detection, writeback persist, and Graph API upload. On slow networks or large files, this blocks the application that called `close()` and ties up OS callback threads. Additionally, uploads have no concurrency control, no deduplication across rapid saves, and no centralized observability. The app is pre-production, making this the right time to establish a solid async upload foundation before adding features on top.

## What Changes

- New `SyncProcessor` module in `carminedesktop-vfs` — a single tokio task that owns all upload lifecycle: debounce, dedup by inode, concurrency-bounded upload spawning, retry with backoff, and crash recovery on startup.
- `flush_handle()` returns immediately after persisting to the writeback cache, sending a `SyncRequest::Flush` to the processor instead of uploading inline.
- `flush_inode()` extracted from `CoreOps` method to a free function callable by both `CoreOps` (fallback) and the processor.
- The `retry_pending_writes` background task (15s interval in `main.rs`) is removed — the processor's tick absorbs retry responsibility.
- Shutdown sequence updated: `SyncRequest::Shutdown` drains in-flight uploads with a configurable timeout before unmount.
- `SyncMetrics` exposed via `watch` channel for queue depth, in-flight count, dedup stats, and error counts.

## Capabilities

### New Capabilities
- `sync-processor`: Channel-based async upload processor with debounce, dedup, bounded concurrency, retry, crash recovery, and observability metrics.

### Modified Capabilities
- `virtual-filesystem`: `flush_handle()` becomes non-blocking (delegates to sync processor instead of uploading inline). `flush_inode()` extracted to free function. Pending write retry task removed from app lifecycle.

## Impact

- **Code**: `crates/carminedesktop-vfs/src/` — new `sync_processor.rs`, changes to `core_ops.rs` (extract `flush_inode`, add `SyncHandle` to `CoreOps`), changes to `pending.rs` (crash recovery reuse).
- **Code**: `crates/carminedesktop-app/src/main.rs` — processor spawned in `start_mount`, shutdown in `stop_mount`, `retry_pending_writes` task removed.
- **Backends**: FUSE and WinFsp backends unchanged — they already delegate to `CoreOps::flush_handle()`.
- **Behavior change**: `flush`/`close` returns before upload completes. Applications see success immediately; upload failures surface via `VfsEvent` notifications and are retried by the processor.
- **Dependencies**: No new crate dependencies (uses existing `tokio::sync::mpsc`, `tokio::sync::Semaphore`, `tokio::sync::watch`).
