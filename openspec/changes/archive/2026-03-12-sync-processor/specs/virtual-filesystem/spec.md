## MODIFIED Requirements

### Requirement: File write operations
The system SHALL buffer writes in the `OpenFile` content buffer associated with the file handle and flush to the writeback buffer on `flush`/`release`. Writing to a file with an in-progress streaming download SHALL block until the download completes. On flush, the system SHALL persist content to the writeback cache and delegate upload to the `SyncProcessor` instead of uploading inline.

#### Scenario: Write to a file
- **WHEN** a write operation is issued with a valid file handle whose content is fully available
- **THEN** the system mutates the handle's `OpenFile` content buffer in-place at the specified offset, marks the handle as dirty, updates the in-memory metadata size, and returns success immediately without touching the writeback buffer

#### Scenario: Write to a file with in-progress download
- **WHEN** a write operation is issued with a valid file handle whose content is still being downloaded
- **THEN** the system blocks until the background download completes, transitions the download state to complete, then performs the write as normal

#### Scenario: Flush on file close
- **WHEN** a file with pending writes is closed (release/flush) and a `SyncHandle` is available
- **THEN** the system pushes the `OpenFile` buffer content to the writeback buffer, persists it to disk for crash safety, sends a `SyncRequest::Flush { ino }` to the sync processor, and returns success immediately without waiting for the upload to complete

#### Scenario: Flush on file close without sync processor
- **WHEN** a file with pending writes is closed (release/flush) and no `SyncHandle` is available (tests or processor disabled)
- **THEN** the system pushes the `OpenFile` buffer content to the writeback buffer, then uploads the complete modified file to the Graph API synchronously using the appropriate upload method (small or chunked), and updates the local metadata with the new eTag

#### Scenario: Write conflict detected
- **WHEN** uploading a modified file and the remote eTag differs from the local eTag (another user modified the file)
- **THEN** the system saves the local version as `<filename>.conflict.<timestamp>` in the same directory, downloads the remote version as the primary file, and emits a notification about the conflict

### Requirement: Graceful unmount
The system SHALL cleanly unmount drives without data loss. On unmount, the system SHALL send `SyncRequest::Shutdown` to the sync processor and await its completion (with the configured shutdown timeout) before unmounting. The `shutdown_on_signal` function SHALL release the mounts mutex before performing blocking unmount operations to prevent deadlock under concurrent access. After the sync processor exits, the shared `flush_pending()` function SHALL run as a last-resort safety net.

#### Scenario: User-initiated unmount
- **WHEN** the user clicks "Unmount" in the tray app
- **THEN** the system sends `SyncRequest::Shutdown` to the sync processor, waits for it to drain pending and in-flight uploads (up to the configured timeout), then runs `flush_pending()` as a safety net, unmounts the FUSE/WinFsp filesystem, and confirms unmount to the user

#### Scenario: Forced unmount on shutdown
- **WHEN** the system receives a shutdown signal (SIGTERM, system reboot)
- **THEN** the system sends `SyncRequest::Shutdown`, waits for the processor with a 10-second timeout, runs `flush_pending()`, forcefully unmounts the filesystem, and saves any unflushed changes to the write-back buffer for crash recovery on next start

#### Scenario: shutdown_on_signal releases mutex before unmount
- **WHEN** `shutdown_on_signal` is triggered by a signal
- **THEN** the system drains the mount handles out of the mutex (via `std::mem::take`), releases the mutex lock, then iterates through the handles and unmounts each one sequentially
- **AND** other threads can access the (now-empty) mounts collection during the unmount process

## REMOVED Requirements

### Requirement: Pending writes retry background task
**Reason**: The `retry_pending_writes` background task (15-second interval in `main.rs`) is replaced by the SyncProcessor's tick-based retry with exponential backoff. The processor provides superior retry behavior (backoff, max retries, dedup) and eliminates the risk of two systems retrying the same upload simultaneously.
**Migration**: Remove the `retry_pending_writes` task spawn from `start_delta_sync()` in `main.rs`. The SyncProcessor's crash recovery at startup and tick-based retry handle all cases previously covered by this task.
