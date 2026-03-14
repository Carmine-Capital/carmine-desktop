## ADDED Requirements

### Requirement: Synchronous flush request handling
The SyncProcessor SHALL support a `SyncRequest::FlushSync { ino, done: oneshot::Sender<bool> }` variant that bypasses debounce and provides completion notification. When a `FlushSync` request arrives, the processor SHALL immediately spawn the upload task (acquiring a semaphore permit) without inserting the inode into the pending map. The processor SHALL store the oneshot sender alongside the in-flight entry and resolve it with `true` on upload success or `false` on upload failure. If the inode is already in-flight when `FlushSync` arrives, the processor SHALL attach the oneshot to the existing in-flight entry so that the caller receives notification when the current upload completes.

#### Scenario: FlushSync bypasses debounce
- **WHEN** a `FlushSync { ino: 42, done }` request arrives
- **THEN** the processor immediately spawns an upload task for inode 42 without inserting it into the pending map or waiting for a debounce tick
- **AND** the upload uses the same `flush_inode_async` path as regular flushes

#### Scenario: FlushSync signals success
- **WHEN** a `FlushSync { ino: 42, done }` request is processed and the upload succeeds
- **THEN** the processor sends `true` on the oneshot channel and the caller unblocks

#### Scenario: FlushSync signals failure
- **WHEN** a `FlushSync { ino: 42, done }` request is processed and the upload fails
- **THEN** the processor sends `false` on the oneshot channel, the caller unblocks with a failure indication, and the inode is inserted into the failed map for retry as normal

#### Scenario: FlushSync for already in-flight inode
- **WHEN** a `FlushSync { ino: 42, done }` arrives while inode 42 is already being uploaded (from a prior `Flush` or `FlushSync`)
- **THEN** the processor attaches the oneshot to the existing in-flight entry and the caller receives the result when the current upload completes (no duplicate upload is spawned)

#### Scenario: FlushSync respects concurrency limits
- **WHEN** a `FlushSync` arrives and all semaphore permits are in use
- **THEN** the upload task waits for a permit before starting (the caller remains blocked until the upload completes, including the wait for a permit)

## MODIFIED Requirements

### Requirement: SyncProcessor event loop
The system SHALL provide a `SyncProcessor` tokio task in `carminedesktop-vfs` that receives `SyncRequest` messages via an unbounded `tokio::sync::mpsc` channel and processes them in an event loop using `tokio::select!`. The event loop SHALL have three branches in priority order: (1) drain upload results from an internal bounded channel, (2) receive external sync requests, (3) process periodic tick. The processor SHALL own all upload-related state as local variables (no `Mutex`-guarded shared state).

#### Scenario: Processor receives Flush request
- **WHEN** the processor receives `SyncRequest::Flush { ino }` via the request channel
- **THEN** the processor inserts the inode into its pending map with the current timestamp and returns to the event loop without performing any upload

#### Scenario: Processor receives FlushSync request
- **WHEN** the processor receives `SyncRequest::FlushSync { ino, done }` via the request channel
- **THEN** the processor immediately spawns an upload task for the inode (bypassing debounce), stores the oneshot sender with the in-flight entry, and returns to the event loop

#### Scenario: Processor receives Shutdown request
- **WHEN** the processor receives `SyncRequest::Shutdown`
- **THEN** the processor flushes all pending inodes immediately (without debounce), waits for all in-flight uploads to complete (up to the configured shutdown timeout), resolves any outstanding FlushSync oneshot senders, and exits the event loop

#### Scenario: Processor channel closed
- **WHEN** the request channel is closed (all senders dropped)
- **THEN** the processor exits the event loop

#### Scenario: Upload result draining priority
- **WHEN** both an upload result and a new sync request are available simultaneously
- **THEN** the processor processes the upload result first to free the concurrency slot before accepting new work

### Requirement: Transient file cache cleanup
When the sync processor skips a transient file upload (as defined by the temp-file-upload-filter capability), the system SHALL perform full cache cleanup in addition to removing the writeback entry. The system SHALL remove the inode from the memory cache, remove the child entry from the parent directory's children map, and remove the inode mapping from the InodeTable. This ensures that skipped transient files do not appear as ghost entries in directory listings.

#### Scenario: Transient file fully cleaned from cache
- **WHEN** `flush_inode_async` detects that a file matches a transient pattern (e.g., `~$Report.xlsx`)
- **THEN** the system removes the writeback entry, removes the inode from the memory cache via `cache.memory.remove(ino)`, removes the child from the parent's children map via `cache.memory.remove_child(parent_ino, &item.name)`, and removes the inode mapping via `inodes.remove_by_item_id(&item_id)`
- **AND** subsequent directory listings for the parent directory do not include the transient file

#### Scenario: Transient file cleanup with missing parent inode
- **WHEN** `flush_inode_async` cleans up a transient file but the parent inode cannot be resolved from the parent item_id (e.g., parent directory was deleted)
- **THEN** the system skips the `remove_child` call, still removes the memory cache entry and inode mapping, and logs a debug-level message

#### Scenario: Non-transient file unaffected by cleanup
- **WHEN** `flush_inode_async` processes a normal file (not matching transient patterns)
- **THEN** the system proceeds with the standard upload path and does not perform any cache cleanup
