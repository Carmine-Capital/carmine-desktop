## Purpose
Defines the runtime lifecycle of the carminedesktop application: component initialization, mount management, authentication state handling, crash recovery, and graceful shutdown.
## Requirements
### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load user config → derive effective config with built-in defaults, (4) run pre-flight validation (client ID sanity check, FUSE availability on Linux/macOS, WinFsp version on Windows), (5) create AuthManager with the official carminedesktop client_id (overridden by `--client-id` CLI arg if provided), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components, (10) register `tauri-plugin-updater` in the Tauri builder (desktop mode only)

#### Scenario: Initialization failure in config
- **WHEN** the user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID or unsupported Windows version)
- **THEN** on Windows release desktop builds, the system displays a `MessageBoxW` dialog with the full actionable error message and exits with code 1; on all other platforms and build configurations, the system prints the actionable error message to stderr and exits with code 1

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
- **THEN** the system shows the sign-in wizard and waits for user authentication, regardless of whether this is the first run or a subsequent launch after sign-out

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
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state. The `start_mount` function SHALL extract shared initialization logic (drive validation, cache directory resolution, CacheManager creation, InodeTable setup, event channel wiring, state insertion) into a platform-agnostic helper, with only the platform-specific mount handle construction remaining in cfg-gated code. On Windows, the `account_name` parameter passed to `WinFspMountHandle::mount()` SHALL be the mount configuration's display name (not the Graph API drive ID).

`start_mount` SHALL NOT send notifications — the caller is responsible for notification dispatch. This enables different notification strategies: batch summaries for startup (`start_all_mounts`) and per-mount notifications for user-initiated actions (`add_mount`, `toggle_mount`).

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, detects and cleans up any stale FUSE mount at the target path, creates the mount point directory if it does not exist, starts a FUSE or WinFsp session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list
- **AND** the function returns success without sending any notification

#### Scenario: Start mount failure — root resolution
- **WHEN** the system attempts to start a mount but the drive root item cannot be fetched from the Graph API
- **THEN** the mount is skipped, an error is logged with the drive name and reason, no notification is sent, and other mounts continue unaffected

#### Scenario: Start mount — stale FUSE mount detected
- **WHEN** the system attempts to create or access the mount point directory and the path is a stale FUSE mount (stat returns ENOTCONN or EIO)
- **THEN** the system attempts to clean up the stale mount via `fusermount -u` (or `umount` on macOS), logs the cleanup result, and retries directory creation; if cleanup fails, the mount is skipped with an actionable error message suggesting manual `fusermount -u <path>`

#### Scenario: Start mount passes correct account_name on Windows
- **WHEN** the system starts a WinFsp mount on Windows
- **THEN** the `account_name` parameter passed to `WinFspMountHandle::mount()` is the mount configuration's human-readable display name (e.g., "OneDrive - Contoso"), NOT the Graph API drive ID
- **AND** the `account_name` is sanitized by replacing `!` characters with `_` per the sync root ID spec

#### Scenario: Start mount uses shared initialization helper
- **WHEN** the system starts a mount on any platform
- **THEN** the shared helper performs: drive validation, cache directory resolution, CacheManager creation, InodeTable setup, event channel creation, and state insertion
- **AND** only the final mount handle construction (FUSE `MountHandle` or WinFsp `WinFspMountHandle`) is platform-specific

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or WinFsp session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication — batch notification
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id, root resolution failure, unrecoverable stale mount), logs skipped mounts with the reason
- **AND** after all mount attempts complete, sends ONE summary notification:
  - If all succeeded: "N drives mounted"
  - If some failed: "N drives mounted, M failed"
  - If all failed: "Failed to mount N drives"

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system SHALL, in order: (1) attempt to stop all active mounts (best-effort, errors logged but not fatal), (2) attempt to clear authentication tokens from secure storage, remove account metadata from user config, and save the config (best-effort, errors logged), (3) regardless of any failures in steps 1-2, set the authenticated flag to false, rebuild the tray menu to the unauthenticated state, reload the settings window to clean DOM state, and show the sign-in wizard; if any step in phase 1-2 produced an error, the system SHALL emit a desktop notification describing the failure

#### Scenario: Mount config change — per-mount notification
- **WHEN** the user adds or enables a mount via the UI (`add_mount` or `toggle_mount` command)
- **THEN** the system applies the change immediately and sends a per-mount "Mount Ready" notification (not a batch summary), since the user explicitly initiated this single operation

#### Scenario: Mount config change — disable or remove
- **WHEN** the user disables or removes a mount via the UI
- **THEN** the system applies the change immediately without a success notification (stopping a mount is silent)

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

### Requirement: Update checker lifecycle
The system SHALL start a background update checker after initialization in desktop mode, and cancel it during shutdown.

#### Scenario: Start update checker after mounts
- **WHEN** the application completes initialization and mount startup in desktop mode
- **THEN** the system spawns a background task that waits 10 seconds, performs an initial update check, then checks every 4 hours

#### Scenario: Cancel update checker on shutdown
- **WHEN** the application begins graceful shutdown (quit, signal, or restart-to-update)
- **THEN** the system cancels the periodic update checker task before proceeding with mount teardown

#### Scenario: Restart-to-update shutdown
- **WHEN** the user triggers "Restart to Update" from the tray menu
- **THEN** the system performs the standard graceful shutdown sequence (cancel sync, flush pending writes with 30-second timeout, unmount all drives), then delegates to the Tauri updater plugin to install the update and relaunch the process

### Requirement: Graceful shutdown
The system SHALL perform an ordered shutdown to prevent data loss.

#### Scenario: Quit from tray menu
- **WHEN** the user selects "Quit" from the tray context menu
- **THEN** the system stops the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/WinFsp sessions, closes database connections, and exits the process

#### Scenario: System signal (SIGTERM, Ctrl+C)
- **WHEN** the process receives SIGTERM or SIGINT
- **THEN** the system performs the same ordered shutdown as the "Quit" action; specifically, the signal handler SHALL be registered during application setup so that it invokes the graceful shutdown sequence including delta sync cancellation, pending write flush, and mount teardown

#### Scenario: Flush timeout exceeded
- **WHEN** pending writes cannot be flushed within the 30-second timeout during shutdown
- **THEN** the system logs a warning with the number of unflushed writes, forcefully unmounts, and exits; unflushed writes remain in the writeback buffer for recovery on next startup

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. The system SHALL also support running in headless mode with the `desktop` feature when `--headless` is passed. On Windows, headless mode SHALL exit with a clear error message instead of silently running as an idle process.

On Windows, headless mode is supported via WinFsp. The `run_headless` function SHALL NOT reject Windows builds. WinFsp mounts work without a desktop session because `FileSystemHost::start()` operates independently of Explorer or any GUI components.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "carminedesktop headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if the browser cannot be opened (no display server), the system prints the auth URL to stdout for manual copy-paste; if sign-in fails after all attempts, the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (user config → effective config with built-in defaults), with the same CLI/env override chain, without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/WinFsp sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during operation
- **THEN** the system logs a warning "Re-authentication required — cached files remain accessible", keeps all mounts alive in degraded mode (cached reads succeed, uncached reads fail with I/O error, writes buffer locally), and continues running until explicitly terminated

#### Scenario: Headless mode runs as foreground process
- **WHEN** the application starts in headless mode
- **THEN** the process SHALL remain in the foreground (not daemonize), blocking on a signal wait after completing initialization; all log output goes to stderr via the tracing subscriber

#### Scenario: Headless mode on Windows
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system initializes WinFsp mounts, starts delta sync, and runs until terminated by a signal
- **AND** the mounted drives are accessible by all processes on the system

#### Scenario: Headless via --headless flag
- **WHEN** the application is compiled with the `desktop` feature and started with `--headless`
- **THEN** the system runs in headless mode, bypassing Tauri initialization, using the same headless startup sequence as a non-desktop build

