## Context

CloudMount's VFS layer (FUSE on Linux/macOS, WinFsp on Windows) processes file uploads synchronously on the OS callback thread. When an application closes a modified file, `flush_handle()` blocks until the entire upload pipeline completes: writeback persist, conflict detection (Graph API call), and upload (Graph API call). This ties up OS threads and blocks applications unnecessarily.

Both FUSE and WinFsp backends delegate all filesystem logic to `CoreOps` in `core_ops.rs` (1856 lines). The upload path (`flush_inode`, lines 913-1104) lives inside `CoreOps` as an async method. A separate 15-second retry task in `main.rs` handles failed uploads by scanning the writeback cache.

There is no concurrency control, deduplication, or centralized observability for uploads.

## Goals / Non-Goals

**Goals:**
- `flush_handle()` returns immediately after persisting to writeback cache — no upload on the VFS thread
- Single async processor task owns all upload lifecycle: debounce, dedup, bounded concurrency, retry, shutdown
- Crash recovery at processor startup drains the writeback cache
- Observability via `SyncMetrics` (queue depth, in-flight, errors, dedup stats)
- Zero behavior change for the FUSE/WinFsp backends — they continue calling `CoreOps::flush_handle()`

**Non-Goals:**
- Delta sync integration into the processor (future iteration)
- Pause/resume sync (infrastructure exists but no trigger mechanism)
- Upload batching or priority queues (future optimization)
- Bounded channel with backpressure signaling
- UI exposure of metrics (Tauri command added separately)

## Decisions

### 1. Processor as a separate module (`sync_processor.rs`)

**Decision**: New file `crates/cloudmount-vfs/src/sync_processor.rs`, not added to `core_ops.rs`.

**Alternatives considered**:
- *Add to `core_ops.rs`*: Already 1856 lines. Adding ~300 lines of processor logic would make it harder to reason about. The processor has a distinct responsibility (upload scheduling) vs `core_ops` (filesystem operations).
- *Put in `cloudmount-app`*: Would break crate boundaries — VFS should own its upload pipeline without depending on the app layer.

**Rationale**: Single-responsibility module. The processor is a self-contained unit with a clear interface (`SyncHandle` to send, `spawn_sync_processor()` to start). Testable independently with mocked dependencies.

### 2. Extract `flush_inode` to a free function

**Decision**: `flush_inode()` becomes `pub(crate) async fn flush_inode(ino, graph, cache, inode_table, event_tx)` — a free function taking dependencies as parameters.

**Alternatives considered**:
- *Keep as `CoreOps` method, processor holds `Arc<CoreOps>`*: Would give the processor access to all of `CoreOps`, violating least-privilege. The processor only needs `flush_inode`'s dependencies, not the full `CoreOps` surface.
- *Trait-based abstraction*: Over-engineering for a crate-internal function.

**Rationale**: Both `CoreOps` (for fallback path) and the processor call the same function. No duplication. Dependencies are already `Arc`-wrapped in the existing code.

### 3. Two channels: requests (external) + results (internal)

**Decision**:
- `UnboundedSender<SyncRequest>` for external requests (flush, shutdown)
- `bounded Sender<UploadResult>` (capacity = `max_concurrent_uploads`) for internal upload completion feedback

**Alternatives considered**:
- *Single channel for both*: Leaks internal `UploadResult` variant into the public `SyncRequest` enum. Cannot prioritize result draining over new requests.

**Rationale**: Clean public API (`Flush` + `Shutdown` only). The processor's `select!` drains results first, ensuring in-flight slots are freed before accepting new work.

### 4. Debounce in the processor, not the watcher/VFS

**Decision**: No per-path debounce at the event source. The processor debounces globally by inode with a configurable delay (default 500ms).

**Rationale**: Multiple sources can trigger flush for the same inode (explicit `flush()`, `release()` of dirty handle). Processor-level dedup ensures at most one upload per inode regardless of how many sources report changes.

### 5. `SyncHandle` is optional in `CoreOps`

**Decision**: `CoreOps` holds `Option<SyncHandle>`. When `None`, `flush_handle()` falls back to synchronous inline upload (current behavior).

**Rationale**: Tests create `CoreOps` without a processor. No test changes needed for existing behavior. Gradual migration — the processor can be disabled for debugging.

### 6. Metrics via `watch` channel

**Decision**: Processor updates `SyncMetrics` at each tick via `watch::Sender`. `SyncHandle::metrics()` reads the latest snapshot.

**Alternatives considered**:
- *`Arc<AtomicU64>` counters*: Fine-grained but no snapshot consistency — readers see partially updated state.
- *Request-response via channel*: Adds latency, the processor must handle metric requests in its event loop.

**Rationale**: `watch` gives snapshot consistency (all fields from the same tick), zero contention (single writer), and the latest value is always available without blocking the processor.

### 7. Absorb `retry_pending_writes` into the processor

**Decision**: The 15-second retry task in `main.rs` is removed. The processor's tick handles retries with exponential backoff.

**Rationale**: Two systems retrying uploads creates race conditions (both try to upload the same file). Single owner of retry logic is simpler and safer. Crash recovery at startup handles the cold-start case.

## Risks / Trade-offs

**[Behavior change: flush returns before upload]** Applications calling `close()` get success before the file is uploaded. If the upload fails, the user learns via notification, not via `close()` error code.
-> Mitigation: Writeback cache persists content to disk before returning. Failed uploads retry automatically. `VfsEvent::UploadFailed` surfaces to the user via desktop notification. This matches the behavior model of cloud sync clients (OneDrive, Dropbox).

**[Processor single point of failure]** If the processor task panics, uploads stop.
-> Mitigation: `SyncHandle::send()` detects a closed channel and logs a warning. Content remains in writeback cache. `flush_pending()` at unmount acts as a last-resort safety net. A future enhancement could add supervisor restart.

**[Dedup window can delay uploads]** The 500ms debounce delays upload start.
-> Mitigation: 500ms is short enough to batch rapid saves, long enough to avoid duplicate uploads. Configurable via `SyncProcessorConfig::debounce_ms`.

**[Shutdown timeout can lose in-flight uploads]** If shutdown timeout (30s) expires, in-flight uploads are abandoned.
-> Mitigation: Content is already persisted in writeback cache. Crash recovery on next startup will resume. This is the same guarantee as the current system (existing `flush_pending` has a 30s timeout).
