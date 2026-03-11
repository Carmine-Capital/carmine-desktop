## Purpose

This spec defines the filesystem watcher that detects local file changes within CfApi sync roots on Windows, using `ReadDirectoryChangesW` with debouncing and thread isolation.

## Requirements

### Requirement: Filesystem watcher for local changes in CfApi sync root
On Windows, the system SHALL spawn a dedicated filesystem watcher thread when a CfApi sync root is mounted. The watcher SHALL use `ReadDirectoryChangesW` with `FILE_NOTIFY_CHANGE_FILE_NAME`, `FILE_NOTIFY_CHANGE_DIR_NAME`, `FILE_NOTIFY_CHANGE_SIZE`, and `FILE_NOTIFY_CHANGE_LAST_WRITE` flags to detect file creation, deletion, renaming, content modification, and write timestamp changes within the sync root directory tree. The watcher SHALL monitor the entire directory tree recursively (`bWatchSubtree = TRUE`).

#### Scenario: File created in sync root
- **WHEN** a new file is created in the sync root (e.g., copy-in from outside, application creates a new file)
- **THEN** the watcher detects the `FILE_ACTION_ADDED` event and routes the file's absolute path to `ingest_local_change()` after the debounce window expires

#### Scenario: File content modified
- **WHEN** an existing file in the sync root is modified (content written, size changed)
- **THEN** the watcher detects the `FILE_ACTION_MODIFIED` event for size or last-write change and routes the file's absolute path to `ingest_local_change()` after the debounce window expires

#### Scenario: File renamed
- **WHEN** a file or directory in the sync root is renamed
- **THEN** the watcher detects the `FILE_ACTION_RENAMED_OLD_NAME` and `FILE_ACTION_RENAMED_NEW_NAME` events and routes the new path to `ingest_local_change()` after the debounce window expires

#### Scenario: File deleted
- **WHEN** a file or directory is deleted from the sync root
- **THEN** the watcher detects the `FILE_ACTION_REMOVED` event and logs the deletion path for diagnostic purposes

#### Scenario: Sync root unmounted
- **WHEN** the CfApi sync root is unmounted
- **THEN** the watcher thread terminates cleanly without blocking the unmount operation

### Requirement: Watcher event debouncing
The filesystem watcher SHALL debounce events per file path with a 500ms window. If multiple events arrive for the same path within 500ms, only a single `ingest_local_change()` call SHALL be made after the window expires. The debounce mechanism SHALL use the absolute path as the deduplication key.

#### Scenario: Rapid successive modifications to same file
- **WHEN** a file is modified multiple times within 500ms (e.g., application writes in chunks)
- **THEN** the watcher makes a single `ingest_local_change()` call after 500ms from the last event for that path

#### Scenario: Different files modified simultaneously
- **WHEN** two different files are modified within the same 500ms window
- **THEN** the watcher makes separate `ingest_local_change()` calls for each file, each after its own 500ms debounce window

#### Scenario: Events separated by more than debounce window
- **WHEN** a file is modified, then modified again after more than 500ms
- **THEN** the watcher makes two separate `ingest_local_change()` calls, one for each modification

### Requirement: Watcher thread isolation
The filesystem watcher SHALL run on a dedicated OS thread (`std::thread::spawn`), separate from CfApi callback threads and the Tokio runtime. The watcher thread SHALL hold an `Arc` reference to the `CloudMountCfFilter` to invoke `ingest_local_change()`. The watcher SHALL NOT block or interfere with CfApi callback processing.

#### Scenario: CfApi callback fires during watcher processing
- **WHEN** a CfApi callback (e.g., `closed()`) fires while the watcher is processing an event batch
- **THEN** both execute concurrently without blocking each other

#### Scenario: Watcher error does not crash the mount
- **WHEN** `ReadDirectoryChangesW` returns an error (e.g., buffer overflow)
- **THEN** the watcher logs a warning and retries the `ReadDirectoryChangesW` call rather than terminating the thread
