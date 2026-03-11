## REMOVED Requirements

### Requirement: Filesystem watcher for local changes in CfApi sync root
**Reason**: WinFsp handles all local file changes through its `FileSystemContext` callbacks (`write`, `create`, `rename`, `cleanup`). There is no need for a separate `ReadDirectoryChangesW` watcher thread because WinFsp routes every filesystem operation through CloudMount's userspace callbacks. Local writes are captured in CoreOps' writeback buffer directly via the `write`/`cleanup`/`close` callback chain.
**Migration**: Remove the `spawn_local_watcher()` function and the watcher thread from `CfMountHandle`. WinFsp's `write` callback -> CoreOps -> writeback buffer replaces the watcher -> `ingest_local_change()` pipeline.

### Requirement: Watcher event debouncing
**Reason**: With WinFsp, writes are handled synchronously through callbacks. There are no asynchronous filesystem events to debounce. Each write operation is processed immediately in the `write` callback and flushed on `cleanup`/`close`.
**Migration**: No migration needed. WinFsp callback-based I/O is inherently ordered and does not produce duplicate events.

### Requirement: Watcher thread isolation
**Reason**: The dedicated watcher thread is removed entirely. WinFsp dispatches I/O callbacks on its own internal threads. The `CloudMountWinFsp` struct is `Send + Sync` and handles concurrent callbacks safely through CoreOps' existing synchronization (DashMap, Mutex).
**Migration**: Remove the watcher OS thread, the `stop_flag`, the `watcher_thread_handle`, and the `CancelSynchronousIo` cleanup from `CfMountHandle::unmount()`.
