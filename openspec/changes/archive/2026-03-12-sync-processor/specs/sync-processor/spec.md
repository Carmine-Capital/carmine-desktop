## ADDED Requirements

### Requirement: SyncProcessor event loop
The system SHALL provide a `SyncProcessor` tokio task in `cloudmount-vfs` that receives `SyncRequest` messages via an unbounded `tokio::sync::mpsc` channel and processes them in an event loop using `tokio::select!`. The event loop SHALL have three branches in priority order: (1) drain upload results from an internal bounded channel, (2) receive external sync requests, (3) process periodic tick. The processor SHALL own all upload-related state as local variables (no `Mutex`-guarded shared state).

#### Scenario: Processor receives Flush request
- **WHEN** the processor receives `SyncRequest::Flush { ino }` via the request channel
- **THEN** the processor inserts the inode into its pending map with the current timestamp and returns to the event loop without performing any upload

#### Scenario: Processor receives Shutdown request
- **WHEN** the processor receives `SyncRequest::Shutdown`
- **THEN** the processor flushes all pending inodes immediately (without debounce), waits for all in-flight uploads to complete (up to the configured shutdown timeout), and exits the event loop

#### Scenario: Processor channel closed
- **WHEN** the request channel is closed (all senders dropped)
- **THEN** the processor exits the event loop

#### Scenario: Upload result draining priority
- **WHEN** both an upload result and a new sync request are available simultaneously
- **THEN** the processor processes the upload result first to free the concurrency slot before accepting new work

### Requirement: SyncProcessor debounce and deduplication
The processor SHALL debounce flush requests by inode. When a `Flush { ino }` arrives for an inode already in the pending map, the processor SHALL update the timestamp to the current time (resetting the debounce window). On each tick, the processor SHALL flush only inodes whose last event timestamp is older than the configured debounce duration (default 500ms). An inode that is currently in-flight SHALL NOT be re-enqueued; the in-flight upload will read the latest content from the writeback cache.

#### Scenario: Rapid saves to same file deduplicated
- **WHEN** 10 `Flush { ino: 42 }` requests arrive within 500ms
- **THEN** the processor performs exactly 1 upload for inode 42 after the debounce window expires

#### Scenario: Debounce window reset on new event
- **WHEN** a `Flush { ino: 42 }` arrives 400ms after the previous one for the same inode
- **THEN** the debounce timer resets and the upload is deferred by another 500ms from the new event

#### Scenario: In-flight inode receives new flush
- **WHEN** a `Flush { ino: 42 }` arrives while inode 42 is already being uploaded
- **THEN** the processor does NOT enqueue a second upload; the current upload reads from writeback cache which contains the latest content

#### Scenario: Different inodes processed independently
- **WHEN** `Flush { ino: 42 }` and `Flush { ino: 99 }` arrive within the same tick
- **THEN** both are debounced and uploaded independently according to their own timestamps

### Requirement: SyncProcessor bounded concurrency
The processor SHALL limit the number of concurrent uploads using a `tokio::sync::Semaphore` with a configurable permit count (default 4). Each upload task SHALL acquire a permit before starting and release it on completion. The processor SHALL spawn uploads as `tokio::spawn` tasks that send their result back via the internal bounded result channel.

#### Scenario: Concurrency limit respected
- **WHEN** 8 inodes are ready for upload and `max_concurrent_uploads` is 4
- **THEN** the processor spawns 4 upload tasks immediately; the remaining 4 wait until permits become available

#### Scenario: Upload task completes and frees slot
- **WHEN** an upload task completes (success or failure)
- **THEN** the semaphore permit is released and the processor removes the inode from the in-flight set

#### Scenario: Upload success
- **WHEN** an upload task calls `flush_inode()` and it returns `Ok(())`
- **THEN** the task sends a success result via the result channel, the processor removes the inode from in-flight, and increments `total_uploaded` in metrics

#### Scenario: Upload failure with retryable error
- **WHEN** an upload task calls `flush_inode()` and it returns a retryable error (network, 5xx, timeout)
- **THEN** the task sends a failure result, the processor removes the inode from in-flight, inserts it into the failed map with an exponential backoff delay, and increments `total_failed` in metrics

### Requirement: SyncProcessor retry with backoff
The processor SHALL retry failed uploads with exponential backoff. Each failed upload SHALL be stored with a retry count and a `next_retry` timestamp. On each tick, the processor SHALL re-enqueue failed uploads whose `next_retry` has passed. The backoff delays SHALL be: 2s, 4s, 8s, 16s, 30s (capped). After 10 consecutive failures for the same inode, the processor SHALL stop retrying and log an error; the content remains in the writeback cache for manual recovery or crash recovery on next startup.

#### Scenario: First retry after failure
- **WHEN** an upload for inode 42 fails for the first time
- **THEN** the processor schedules retry in 2 seconds

#### Scenario: Exponential backoff increases
- **WHEN** an upload for inode 42 fails for the 3rd consecutive time
- **THEN** the processor schedules retry in 8 seconds (2^3)

#### Scenario: Backoff capped at 30 seconds
- **WHEN** an upload for inode 42 fails for the 6th time
- **THEN** the processor schedules retry in 30 seconds (capped, not 64s)

#### Scenario: Max retries exceeded
- **WHEN** an upload for inode 42 fails 10 consecutive times
- **THEN** the processor removes the inode from the failed map, logs an error with the file path and last error, and does NOT schedule further retries

#### Scenario: Successful upload resets retry count
- **WHEN** an upload for inode 42 succeeds after 3 previous failures
- **THEN** the inode is removed from the failed map and its retry count is reset

### Requirement: SyncProcessor crash recovery
On startup, the processor SHALL scan the writeback cache for all persisted entries and enqueue a `Flush` for each one. This recovers uploads that were persisted but not completed before a previous shutdown or crash.

#### Scenario: Pending writes found at startup
- **WHEN** the processor starts and the writeback cache contains 3 persisted entries
- **THEN** the processor enqueues 3 flush requests (one per entry) and processes them through the normal debounce and upload pipeline

#### Scenario: No pending writes at startup
- **WHEN** the processor starts and the writeback cache is empty
- **THEN** the processor enters the event loop immediately without enqueuing any requests

#### Scenario: Recovery entry for unknown inode
- **WHEN** the processor finds a writeback entry whose drive_id/item_id cannot be resolved to an inode (e.g., orphaned `local:*` entry)
- **THEN** the processor logs a warning and skips the entry; the content remains in writeback cache for manual inspection

### Requirement: SyncProcessor graceful shutdown
The processor SHALL support graceful shutdown via `SyncRequest::Shutdown`. On receiving shutdown, the processor SHALL: (1) stop accepting new flush requests, (2) immediately flush all pending inodes without debounce, (3) wait for all in-flight uploads to complete, subject to a configurable timeout (default 30 seconds). If the timeout expires, the processor SHALL log a warning with the count of uploads still in-flight and exit; content remains in the writeback cache.

#### Scenario: Clean shutdown with pending and in-flight uploads
- **WHEN** shutdown is received with 5 pending and 2 in-flight uploads, and all complete within 30 seconds
- **THEN** the processor uploads all 7 files and exits cleanly

#### Scenario: Shutdown timeout exceeded
- **WHEN** shutdown is received and 2 uploads are still in-flight after 30 seconds
- **THEN** the processor logs a warning ("2 uploads still in-flight at shutdown, content preserved in writeback cache") and exits

#### Scenario: Flush received after shutdown
- **WHEN** a `SyncRequest::Flush` arrives after `Shutdown` has been received
- **THEN** the processor ignores it; the content is already persisted in the writeback cache and will be recovered on next startup

### Requirement: SyncHandle interface
The system SHALL provide a `SyncHandle` struct that is `Clone + Send + Sync`. `SyncHandle::send(req)` SHALL send a `SyncRequest` to the processor. If the processor's channel is closed, `send()` SHALL log a warning and return without error. `SyncHandle::metrics()` SHALL return the latest `SyncMetrics` snapshot via a `watch::Receiver`.

#### Scenario: Send flush request
- **WHEN** `sync_handle.send(SyncRequest::Flush { ino: 42 })` is called
- **THEN** the request is sent to the processor's channel and the call returns immediately

#### Scenario: Send to dead processor
- **WHEN** `sync_handle.send(...)` is called after the processor has exited
- **THEN** the method logs a warning at `warn` level and returns without panicking

#### Scenario: Read metrics
- **WHEN** `sync_handle.metrics()` is called
- **THEN** it returns the latest `SyncMetrics` snapshot without blocking the processor

### Requirement: SyncMetrics observability
The processor SHALL maintain and expose a `SyncMetrics` struct updated at each tick via a `watch::Sender`. The metrics SHALL include: `queue_depth` (pending map size), `in_flight` (active upload count), `failed_count` (retry queue size), `total_uploaded` (cumulative successful uploads since startup), `total_failed` (cumulative failures including retries), and `total_deduplicated` (flush requests absorbed by dedup).

#### Scenario: Metrics reflect current state
- **WHEN** the processor has 3 pending, 2 in-flight, and 1 failed upload
- **THEN** `metrics()` returns `queue_depth: 3, in_flight: 2, failed_count: 1`

#### Scenario: Dedup counter incremented
- **WHEN** a `Flush { ino: 42 }` arrives while inode 42 is already pending
- **THEN** `total_deduplicated` is incremented by 1

#### Scenario: Metrics updated each tick
- **WHEN** the processor's tick fires
- **THEN** the `watch::Sender` is updated with the latest metrics snapshot and all `watch::Receiver` holders see the new values

### Requirement: SyncProcessorConfig
The system SHALL provide a `SyncProcessorConfig` struct with the following configurable parameters: `max_concurrent_uploads` (default 4), `debounce_ms` (default 500), `tick_interval_ms` (default 1000), `shutdown_timeout_secs` (default 30). All parameters SHALL have sensible defaults via `Default` trait implementation.

#### Scenario: Default configuration
- **WHEN** `SyncProcessorConfig::default()` is called
- **THEN** it returns `max_concurrent_uploads: 4, debounce_ms: 500, tick_interval_ms: 1000, shutdown_timeout_secs: 30`

#### Scenario: Custom configuration
- **WHEN** a `SyncProcessorConfig` is created with `max_concurrent_uploads: 8`
- **THEN** the processor uses 8 semaphore permits for upload concurrency
