## MODIFIED Requirements

### Requirement: Graceful shutdown
The system SHALL perform an ordered shutdown to prevent data loss.

#### Scenario: Quit from tray menu
- **WHEN** the user selects "Quit" from the tray context menu
- **THEN** the system stops the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, closes database connections, and exits the process

#### Scenario: System signal (SIGTERM, Ctrl+C)
- **WHEN** the process receives SIGTERM or SIGINT
- **THEN** the system performs the same ordered shutdown as the "Quit" action; specifically, the signal handler SHALL be registered during application setup so that it invokes the graceful shutdown sequence including delta sync cancellation, pending write flush, and mount teardown

#### Scenario: Flush timeout exceeded
- **WHEN** pending writes cannot be flushed within the 30-second timeout during shutdown
- **THEN** the system logs a warning with the number of unflushed writes, forcefully unmounts, and exits; unflushed writes remain in the writeback buffer for recovery on next startup

### Requirement: Authentication failure degradation
The system SHALL degrade gracefully when authentication fails during operation, preserving access to cached data.

#### Scenario: Refresh token revoked during operation
- **WHEN** the authentication token cannot be refreshed because the refresh token was revoked (admin action, password change)
- **THEN** the system sets a degraded state flag, updates the tray icon to show a warning indicator, sends a notification "Re-authentication required — cached files remain accessible", and keeps all mounts alive

#### Scenario: Cached reads during auth degradation
- **WHEN** the system is in auth-degraded state and a read request is issued for a file that exists in any cache tier (memory, SQLite, or disk)
- **THEN** the read succeeds normally, serving data from the cache

#### Scenario: Uncached reads during auth degradation
- **WHEN** the system is in auth-degraded state and a read request is issued for a file not in any cache tier
- **THEN** the read fails with an I/O error (the file cannot be fetched without authentication)

#### Scenario: Writes during auth degradation
- **WHEN** the system is in auth-degraded state and a write is issued
- **THEN** the write succeeds locally (data is stored in the writeback buffer), but the flush to the server fails; the pending write is preserved for upload after re-authentication

#### Scenario: Recovery from auth degradation
- **WHEN** the user re-authenticates while the system is in auth-degraded state
- **THEN** the system clears the degraded flag, updates the tray icon to normal, triggers an immediate delta sync for all drives (without waiting for the next scheduled interval), and flushes all pending writes from the writeback buffer by invoking crash recovery

## ADDED Requirements

### Requirement: Immediate sync after authentication
The system SHALL run a delta sync pass immediately when authentication is established or restored, rather than waiting for the next scheduled interval.

#### Scenario: First sync after startup with restored tokens
- **WHEN** the application starts and tokens are successfully restored from secure storage
- **THEN** the system runs a delta sync for all mounted drives immediately after mounts are started, before the periodic sync timer begins its first sleep interval

#### Scenario: First sync after sign-in
- **WHEN** the user completes sign-in (initial or re-authentication)
- **THEN** the system runs a delta sync for all mounted drives immediately, so the cache reflects remote state within seconds of authentication
