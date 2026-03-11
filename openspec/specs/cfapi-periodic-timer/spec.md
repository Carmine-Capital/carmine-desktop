## Purpose

This spec defines the periodic timer thread that processes deferred operations (safe-save renames, deferred ingests) on a 500ms interval within CfApi sync roots on Windows.

## Requirements

### Requirement: Periodic timer for deferred operation processing
On Windows, the system SHALL spawn a background timer thread when a CfApi sync root is mounted. The timer SHALL wake every 500ms and execute the following operations in order:
1. `process_safe_save_timeouts()` -- commit any safe-save deferred renames whose timeout (2 seconds) has expired
2. `process_deferred_timeouts()` -- remove any deferred ingest entries whose TTL (30 seconds) has expired
3. `retry_deferred_ingest()` -- retry ingestion for any deferred entries that have not yet expired

#### Scenario: Safe-save rename expires while no callbacks fire
- **WHEN** a safe-save rename is deferred with a 2-second timeout and no CfApi callbacks fire within that period
- **THEN** the timer thread detects the expired timeout on its next 500ms wake, commits the rename via `core.rename()`, and removes the entry from the safe-save queue

#### Scenario: Deferred ingest retried by timer
- **WHEN** a file ingestion was deferred because metadata was temporarily unavailable (e.g., file still being written)
- **THEN** the timer thread retries `ingest_local_change()` for that path on the next 500ms wake cycle

#### Scenario: Deferred ingest TTL expired
- **WHEN** a deferred ingest entry has been in the queue for more than 30 seconds without successful retry
- **THEN** the timer thread removes the entry from the deferred queue and logs a warning

#### Scenario: Timer thread terminates on unmount
- **WHEN** the CfApi sync root is unmounted
- **THEN** the timer thread terminates cleanly within one timer interval (500ms) without blocking the unmount

### Requirement: Timer thread independence from CfApi callbacks
The timer thread SHALL process deferred operations independently of CfApi callback activity. The timer SHALL NOT depend on `state_changed()`, `closed()`, or `rename()` callbacks firing in order to process expired timeouts or retry deferred ingests.

#### Scenario: No CfApi callbacks fire for 30 seconds
- **WHEN** a file is created in the sync root and no CfApi callbacks fire for 30 seconds afterward
- **THEN** the timer thread still processes the safe-save timeout (if applicable) and retries any deferred ingests during that period

#### Scenario: Timer and callback both process same timeout
- **WHEN** the timer thread and a CfApi callback both attempt to process the same safe-save timeout concurrently
- **THEN** the Mutex-protected queue ensures only one succeeds; the other finds the entry already removed and takes no action
