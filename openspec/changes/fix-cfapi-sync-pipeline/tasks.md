## 1. Fix Unmodified Guard for local:* Items (Bug #1)

- [x] 1.1 In `stage_writeback_from_disk()` in `cfapi.rs`, add a check before the mtime/size comparison: if the item ID starts with `local:`, skip the unmodified guard and always proceed to stage the file for upload
- [x] 1.2 Add a unit/integration test that verifies a `local:*` item is never skipped by the unmodified guard even when mtime and size match the cached DriveItem

## 2. Fix Rename Ticket Acknowledgment (Bug #4)

- [x] 2.1 In the `rename()` callback in `cfapi.rs`, add `ticket.pass()` to the error branch of the `core.rename()` match so the OS always receives acknowledgment regardless of Graph API outcome
- [x] 2.2 Add a log message in the error branch indicating the rename was acknowledged to the OS despite the Graph API failure

## 3. Filesystem Watcher Thread (Bug #2)

- [x] 3.1 Add a `spawn_local_watcher()` function in `cfapi.rs` that creates a `ReadDirectoryChangesW` watcher thread using `windows-sys` with `FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE` flags and recursive monitoring
- [x] 3.2 Implement event parsing for `FILE_ACTION_ADDED`, `FILE_ACTION_MODIFIED`, `FILE_ACTION_RENAMED_OLD_NAME`, `FILE_ACTION_RENAMED_NEW_NAME`, and `FILE_ACTION_REMOVED` from the `FILE_NOTIFY_INFORMATION` buffer
- [x] 3.3 Implement 500ms per-path debouncing using a `HashMap<PathBuf, Instant>` that collapses rapid successive events for the same path into a single `ingest_local_change()` call
- [x] 3.4 Add error handling for `ReadDirectoryChangesW` failures (log warning and retry instead of terminating the thread)
- [x] 3.5 Add a shutdown mechanism (e.g., `Arc<AtomicBool>` flag) so the watcher thread terminates cleanly when the sync root is unmounted

## 4. Periodic Timer Thread (Bugs #3, #5)

- [x] 4.1 Add a `spawn_periodic_timer()` function in `cfapi.rs` that creates a background thread waking every 500ms to call `process_safe_save_timeouts()`, `process_deferred_timeouts()`, and `retry_deferred_ingest()`
- [x] 4.2 Add the same shutdown mechanism as the watcher (shared `Arc<AtomicBool>`) so the timer thread terminates cleanly on unmount

## 5. Mount Lifecycle Integration

- [x] 5.1 Update `CfMountHandle::mount()` to spawn both the watcher and timer threads during mount initialization, storing their `JoinHandle`s for cleanup
- [x] 5.2 Update mount teardown to signal both threads to stop (via the `AtomicBool` flag) and join them before completing unmount

## 6. Post-Upload Placeholder Conversion

- [x] 6.1 In `core_ops.rs`, after `flush_inode()` successfully uploads a `local:*` file and receives a server-assigned item ID, add a `#[cfg(target_os = "windows")]` block that calls `Placeholder::convert_to_placeholder()` with the server item ID as the blob and `mark_in_sync()`
- [x] 6.2 Add error handling for the conversion call (log warning on failure, do not retry)

## 7. Integration Tests

- [x] 7.1 Add a test in `cfapi_integration.rs` for copy-in scenario: a file copied from outside the sync root is detected, ingested, and not skipped by the unmodified guard
- [x] 7.2 Add a test for internal copy scenario: a file copied between subfolders within the sync root triggers ingest for the new copy
- [x] 7.3 Add a test for rename scenario: a renamed file triggers `ticket.pass()` regardless of `core.rename()` outcome
