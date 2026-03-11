## REMOVED Requirements

### Requirement: Periodic timer for deferred operation processing
**Reason**: The periodic timer thread processed CfApi-specific deferred operations: safe-save rename reconciliation, deferred ingest retries, and deferred ingest TTL expiration. WinFsp does not have a deferred processing model — all operations (including renames and writes) are handled synchronously through `FileSystemContext` callbacks. Safe-save detection (`tmp file -> rename to target`) is not needed because WinFsp's `rename` callback delegates directly to `CoreOps::rename()` in real time.
**Migration**: Remove the `spawn_periodic_timer()` function, the `timer_handle` field from `CfMountHandle`, the `safe_save_txns` queue, and the `deferred_ingest` map. WinFsp callbacks handle all operations inline.

### Requirement: Timer thread independence from CfApi callbacks
**Reason**: There is no timer thread with WinFsp. All operations that the timer was responsible for (safe-save commit, deferred ingest retry) are handled directly in WinFsp callbacks or not needed at all.
**Migration**: No migration needed. WinFsp's synchronous callback model eliminates the need for background timer-based processing.
