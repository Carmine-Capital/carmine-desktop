## 1. Types and Configuration

- [x] 1.1 Create `sync_processor.rs` module in `crates/carminedesktop-vfs/src/` with `SyncRequest` enum (`Flush { ino }`, `Shutdown`), `SyncHandle` struct, `SyncProcessorConfig` struct with defaults, `SyncMetrics` struct, and `SyncProcessorDeps` struct
- [x] 1.2 Add `mod sync_processor` to `crates/carminedesktop-vfs/src/lib.rs` and export public types (`SyncHandle`, `SyncRequest`, `SyncProcessorConfig`, `SyncMetrics`, `SyncProcessorDeps`)

## 2. Extract flush_inode

- [x] 2.1 Extract `flush_inode()` from `CoreOps` method to `pub(crate) async fn flush_inode(ino, graph, cache, inode_table, event_tx)` free function in `core_ops.rs`, keeping the original method as a thin wrapper that calls the free function
- [x] 2.2 Verify existing tests pass with the extraction (no behavior change)

## 3. SyncProcessor Core

- [x] 3.1 Implement `spawn_sync_processor(deps, config) -> (SyncHandle, JoinHandle<()>)` — creates channels (unbounded request, bounded result), spawns the processor tokio task, returns the handle
- [x] 3.2 Implement the processor event loop with `tokio::select!` over three branches: result channel (priority), request channel, tick interval
- [x] 3.3 Implement debounce logic: `pending: HashMap<u64, Instant>`, insert/update on `Flush`, flush on tick when debounce window expired
- [x] 3.4 Implement upload spawning with `tokio::sync::Semaphore` for bounded concurrency, `in_flight: HashSet<u64>` tracking, and result reporting via the bounded result channel
- [x] 3.5 Implement result handling: on success remove from in_flight and update metrics; on failure insert into `failed` map with backoff schedule
- [x] 3.6 Implement retry logic: on each tick scan `failed` map for entries past `next_retry`, re-enqueue with exponential backoff (2s, 4s, 8s, 16s, 30s cap), remove after 10 consecutive failures
- [x] 3.7 Implement `SyncMetrics` update via `watch::Sender` at each tick

## 4. Crash Recovery

- [x] 4.1 Implement `recover_pending()` — on processor startup, scan writeback cache for all persisted entries, enqueue a `Flush` for each recoverable entry, log warnings for orphaned `local:*` entries

## 5. Shutdown

- [x] 5.1 Implement `Shutdown` handler: stop accepting new requests, flush all pending immediately (no debounce), wait for in-flight with configurable timeout, log warning if timeout exceeded, break event loop

## 6. Integration with CoreOps

- [x] 6.1 Add `sync_handle: Option<SyncHandle>` field to `CoreOps` struct and its constructor
- [x] 6.2 Modify `flush_handle()` to send `SyncRequest::Flush { ino }` via the sync handle when available, falling back to the existing synchronous `flush_inode()` call when `sync_handle` is `None`

## 7. App Lifecycle Integration

- [x] 7.1 In `start_mount()` (`main.rs`): create `SyncProcessorDeps` from the existing `Arc`-wrapped graph/cache/inode_table, call `spawn_sync_processor()`, pass the `SyncHandle` to `CoreOps`
- [x] 7.2 In `stop_mount()` (`main.rs`): send `SyncRequest::Shutdown`, await the processor `JoinHandle`, then run existing `flush_pending()` as safety net, then unmount
- [x] 7.3 Remove the `retry_pending_writes` background task spawn from `start_delta_sync()` in `main.rs`

## 8. Tests

- [x] 8.1 Unit test: processor debounce — 10 rapid flushes for same ino result in 1 upload call
- [x] 8.2 Unit test: processor concurrency — with `max_concurrent_uploads: 2`, verify at most 2 uploads run simultaneously
- [x] 8.3 Unit test: processor retry with backoff — verify failed upload retries with increasing delay, stops after max retries
- [x] 8.4 Unit test: processor shutdown — pending and in-flight uploads drain before exit
- [x] 8.5 Unit test: crash recovery — processor startup enqueues flushes for writeback cache entries
- [x] 8.6 Unit test: metrics — verify `SyncMetrics` reflects correct queue_depth, in_flight, dedup counts
- [x] 8.7 Integration test: `flush_handle` with sync processor — verify content persisted to writeback and flush request sent (no inline upload)
- [x] 8.8 Verify all existing VFS tests pass (no regressions)

## 9. Cleanup

- [x] 9.1 Run `make clippy` and fix any warnings introduced by the change
- [x] 9.2 Run `make test` to confirm all tests pass
