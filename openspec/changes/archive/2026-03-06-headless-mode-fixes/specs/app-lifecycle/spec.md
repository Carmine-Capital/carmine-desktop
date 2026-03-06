## MODIFIED Requirements

### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Component creation logic (AuthManager, GraphClient, CacheManager, InodeTable) SHALL be shared between desktop and headless modes via a single `init_components()` function to prevent implementation drift.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes components in this order: (1) load config (packaged defaults + user config → effective config), (2) call `init_components()` to create AuthManager with client_id and tenant_id, GraphClient with AuthManager's token provider, CacheManager with cache directory and SQLite database, and shared InodeTable, (3) assemble AppState (desktop) or hold components directly (headless)

#### Scenario: Initialization failure in config
- **WHEN** the packaged defaults or user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

### Requirement: Authentication failure degradation
The system SHALL degrade gracefully when authentication fails during operation, preserving access to cached data. The degradation warning SHALL be logged exactly once per degradation episode, not repeated on each sync cycle.

#### Scenario: Refresh token revoked during operation
- **WHEN** the authentication token cannot be refreshed because the refresh token was revoked (admin action, password change)
- **THEN** the system sets a degraded state flag, updates the tray icon to show a warning indicator (desktop) or logs a warning to stderr (headless), sends a notification "Re-authentication required — cached files remain accessible" (desktop only), and keeps all mounts alive; the warning SHALL be emitted once and not repeated on subsequent sync cycles while the flag remains set

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
- **THEN** the system clears the degraded flag, updates the tray icon to normal (desktop) or logs an info message (headless), triggers an immediate delta sync for all drives (without waiting for the next scheduled interval), and flushes all pending writes from the writeback buffer by invoking crash recovery

### Requirement: Crash recovery
The system SHALL recover pending writes from a previous session that terminated abnormally. Crash recovery SHALL run as a non-blocking background task in both desktop and headless modes, so that mount startup is not delayed by pending-write uploads.

#### Scenario: Pending writes found on startup
- **WHEN** the application starts and the writeback buffer directory contains pending files from a previous session
- **THEN** the system logs the number of pending writes found, and after authentication, spawns a background task that attempts to upload each pending file; mount startup SHALL proceed immediately without waiting for crash recovery to complete

#### Scenario: Crash recovery with conflict
- **WHEN** a pending write from a crashed session is being uploaded and the server's eTag differs from the cached eTag
- **THEN** the system creates a `.conflict.{timestamp}` copy (standard conflict detection behavior) and removes the pending write from the buffer

#### Scenario: Crash recovery with auth failure
- **WHEN** pending writes are found but authentication fails
- **THEN** the system keeps the pending writes in the buffer (they persist on disk) and retries on the next successful authentication

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. Headless sign-in SHALL persist account metadata and auto-discover the user's OneDrive drive, matching desktop behavior.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes as a background task, starts all enabled mounts, starts the periodic delta sync loop, and logs "CloudMount headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if sign-in succeeds, the system proceeds with post-sign-in setup and mount startup; if sign-in fails (browser unavailable, user cancels, timeout), the system logs the error and exits with a non-zero exit code

#### Scenario: Headless post-sign-in setup
- **WHEN** the headless process completes a fresh sign-in (not a token restoration)
- **THEN** the system SHALL call the Microsoft Graph API to discover the user's OneDrive drive, write an AccountMetadata entry (with drive ID and display name) to the user config file, create a default OneDrive mount configuration at `{home}/{root_dir}/OneDrive` if no OneDrive mount exists, save the updated config to disk, and rebuild the effective configuration so the new mount is included in subsequent mount startup

#### Scenario: Headless post-sign-in config write failure
- **WHEN** the headless post-sign-in setup fails to write the config file (permissions error, disk full)
- **THEN** the system logs a warning but continues with mount startup using the in-memory configuration; tokens remain stored in the credential store for next startup

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) via the shared `init_components()` function using the same configuration system (packaged defaults + user config → effective config), without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during the delta sync loop
- **THEN** the system sets a degraded state flag and logs a warning "Re-authentication required — cached files remain accessible" exactly once; subsequent sync cycles SHALL check the flag and skip the warning; all mounts remain alive in degraded mode (cached reads succeed, uncached reads fail with I/O error, writes buffer locally), and the process continues running until explicitly terminated or re-authenticated via SIGHUP

#### Scenario: Headless SIGHUP re-authentication (Unix)
- **WHEN** the headless process receives SIGHUP on a Unix system
- **THEN** the system logs "SIGHUP received — attempting re-authentication", attempts browser-based OAuth sign-in via the PKCE flow; on success, clears the degraded flag, flushes pending writes from the writeback buffer, and logs "re-authentication successful"; on failure (browser unavailable, user cancels, timeout), logs the error and remains in the current state without exiting

#### Scenario: Headless SIGHUP on non-Unix platforms
- **WHEN** the headless process is running on Windows
- **THEN** no SIGHUP handler is registered; re-authentication requires restarting the process

#### Scenario: Headless mode runs as foreground process
- **WHEN** the application starts in headless mode
- **THEN** the process SHALL remain in the foreground (not daemonize), blocking on a signal wait after completing initialization; all log output goes to stderr via the tracing subscriber

#### Scenario: Headless mode on Windows (limitation)
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system logs a warning that Cloud Files API mounts are not supported in headless mode and skips mount startup for affected drives; Windows CfApi headless support is a future enhancement
