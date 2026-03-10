## MODIFIED Requirements

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state. The `start_mount` function SHALL extract shared initialization logic (drive validation, cache directory resolution, CacheManager creation, InodeTable setup, event channel wiring, state insertion, notification dispatch) into a platform-agnostic helper, with only the platform-specific mount handle construction remaining in cfg-gated code. On Windows, the `account_name` parameter passed to `CfMountHandle::mount()` SHALL be the mount configuration's display name (not the Graph API drive ID).

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, detects and cleans up any stale FUSE mount at the target path, creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list, and sends a "Mount Ready" notification

#### Scenario: Start mount failure — root resolution
- **WHEN** the system attempts to start a mount but the drive root item cannot be fetched from the Graph API
- **THEN** the mount is skipped, an error is logged with the drive name and reason, no notification is sent, and other mounts continue unaffected

#### Scenario: Start mount — stale FUSE mount detected
- **WHEN** the system attempts to create or access the mount point directory and the path is a stale FUSE mount (stat returns ENOTCONN or EIO)
- **THEN** the system attempts to clean up the stale mount via `fusermount -u` (or `umount` on macOS), logs the cleanup result, and retries directory creation; if cleanup fails, the mount is skipped with an actionable error message suggesting manual `fusermount -u <path>`

#### Scenario: Start mount passes correct account_name on Windows
- **WHEN** the system starts a CfApi mount on Windows
- **THEN** the `account_name` parameter passed to `CfMountHandle::mount()` is the mount configuration's human-readable display name (e.g., "OneDrive - Contoso"), NOT the Graph API drive ID
- **AND** the `account_name` is sanitized by replacing `!` characters with `_` per the sync root ID spec

#### Scenario: Start mount uses shared initialization helper
- **WHEN** the system starts a mount on any platform
- **THEN** the shared helper performs: drive validation, cache directory resolution, CacheManager creation, InodeTable setup, event channel creation, and state insertion
- **AND** only the final mount handle construction (FUSE `MountHandle` or CfApi `CfMountHandle`) is platform-specific

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or CfApi session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id, root resolution failure, unrecoverable stale mount), and logs skipped mounts with the reason

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system SHALL, in order: (1) attempt to stop all active mounts (best-effort, errors logged but not fatal), (2) attempt to clear authentication tokens from secure storage, remove account metadata from user config, and save the config (best-effort, errors logged), (3) regardless of any failures in steps 1-2, set the authenticated flag to false, rebuild the tray menu to the unauthenticated state, reload the settings window to clean DOM state, and show the sign-in wizard; if any step in phase 1-2 produced an error, the system SHALL emit a desktop notification describing the failure

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. The system SHALL also support running in headless mode with the `desktop` feature when `--headless` is passed. On Windows, headless mode SHALL exit with a clear error message instead of silently running as an idle process.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "CloudMount headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if the browser cannot be opened (no display server), the system prints the auth URL to stdout for manual copy-paste; if sign-in fails after all attempts, the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (user config -> effective config with built-in defaults), with the same CLI/env override chain, without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during operation
- **THEN** the system logs a warning "Re-authentication required — cached files remain accessible", keeps all mounts alive in degraded mode (cached reads succeed, uncached reads fail with I/O error, writes buffer locally), and continues running until explicitly terminated

#### Scenario: Headless mode runs as foreground process
- **WHEN** the application starts in headless mode
- **THEN** the process SHALL remain in the foreground (not daemonize), blocking on a signal wait after completing initialization; all log output goes to stderr via the tracing subscriber

#### Scenario: Headless mode on Windows exits with error
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system prints "Error: headless mode is not supported on Windows. Cloud Files API requires desktop mode. Use 'cloudmount' without --headless." to stderr and exits with exit code 1

#### Scenario: Headless via --headless flag
- **WHEN** the application is compiled with the `desktop` feature and started with `--headless`
- **THEN** the system runs in headless mode, bypassing Tauri initialization, using the same headless startup sequence as a non-desktop build
