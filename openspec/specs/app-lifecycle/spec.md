### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load config (packaged defaults + user config → effective config), (4) run pre-flight validation (client ID, FUSE availability), (5) create AuthManager with resolved client_id and tenant_id (from CLI > env > packaged > default), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components

#### Scenario: Initialization failure in config
- **WHEN** the packaged defaults or user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID)
- **THEN** the system prints an actionable error message to stderr and exits with code 1 before attempting authentication or component initialization

### Requirement: Token restoration on startup
The system SHALL attempt to restore authentication tokens from secure storage on startup before requiring user sign-in.

#### Scenario: Valid tokens found in keyring
- **WHEN** the application starts and an account exists in user config with a stored account_id
- **THEN** the system attempts to load tokens from the OS keyring (or encrypted file fallback), and if valid tokens are found, skips the sign-in flow and proceeds directly to mounting drives

#### Scenario: Expired tokens found
- **WHEN** stored tokens are found but the access token is expired
- **THEN** the system attempts a silent token refresh using the stored refresh token, and if successful, proceeds to mounting drives without user interaction

#### Scenario: No tokens found or refresh fails (desktop mode)
- **WHEN** no stored tokens are found or the refresh token has been revoked, and the application is running in desktop mode
- **THEN** the system shows the sign-in wizard and waits for user authentication

#### Scenario: No tokens found or refresh fails (headless mode)
- **WHEN** no stored tokens are found or the refresh token has been revoked, and the application is running in headless mode
- **THEN** the system attempts browser-based OAuth sign-in via the PKCE flow; if the browser cannot be opened or sign-in fails, the system logs a descriptive error and exits with a non-zero exit code

### Requirement: OneDrive auto-discovery
The system SHALL automatically discover the authenticated user's OneDrive drive after sign-in.

#### Scenario: Discover OneDrive drive ID
- **WHEN** the user successfully authenticates (first-time or re-authentication)
- **THEN** the system calls the Microsoft Graph API (`GET /me/drive`) to retrieve the user's personal OneDrive drive ID, name, and quota information

#### Scenario: OneDrive auto-mount
- **WHEN** the OneDrive drive is discovered and no OneDrive mount exists in the user's config
- **THEN** the system automatically creates a mount configuration for the OneDrive drive at `{home}/{root_dir}/OneDrive` and starts the mount

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state.

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive, adds the drive to the delta sync timer's drive list, and sends a "Mount Ready" notification

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or CfApi session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id), and logs skipped mounts with the reason

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system stops all active mounts, clears authentication tokens from secure storage, removes account metadata from user config, and reverts to the unauthenticated state (showing the wizard on next interaction)

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts

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

### Requirement: Immediate sync after authentication
The system SHALL run a delta sync pass immediately when authentication is established or restored, rather than waiting for the next scheduled interval.

#### Scenario: First sync after startup with restored tokens
- **WHEN** the application starts and tokens are successfully restored from secure storage
- **THEN** the system runs a delta sync for all mounted drives immediately after mounts are started, before the periodic sync timer begins its first sleep interval

#### Scenario: First sync after sign-in
- **WHEN** the user completes sign-in (initial or re-authentication)
- **THEN** the system runs a delta sync for all mounted drives immediately, so the cache reflects remote state within seconds of authentication

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

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. The system SHALL also support running in headless mode with the `desktop` feature when `--headless` is passed.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "CloudMount headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if the browser cannot be opened (no display server), the system prints the auth URL to stdout for manual copy-paste; if sign-in fails after all attempts, the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (packaged defaults + user config → effective config), with the same CLI/env override chain, without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during operation
- **THEN** the system logs a warning "Re-authentication required — cached files remain accessible", keeps all mounts alive in degraded mode (cached reads succeed, uncached reads fail with I/O error, writes buffer locally), and continues running until explicitly terminated

#### Scenario: Headless mode runs as foreground process
- **WHEN** the application starts in headless mode
- **THEN** the process SHALL remain in the foreground (not daemonize), blocking on a signal wait after completing initialization; all log output goes to stderr via the tracing subscriber

#### Scenario: Headless mode on Windows (limitation)
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system logs a warning that Cloud Files API mounts are not supported in headless mode and skips mount startup for affected drives; Windows CfApi headless support is a future enhancement

#### Scenario: Headless via --headless flag
- **WHEN** the application is compiled with the `desktop` feature and started with `--headless`
- **THEN** the system runs in headless mode, bypassing Tauri initialization, using the same headless startup sequence as a non-desktop build
