## MODIFIED Requirements

### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load config (packaged defaults + user config → effective config), (4) run pre-flight validation (client ID, FUSE availability, CfApi version on Windows), (5) create AuthManager with resolved client_id and tenant_id (from CLI > env > packaged > default), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components, (10) register `tauri-plugin-updater` in the Tauri builder (desktop mode only)

#### Scenario: Initialization failure in config
- **WHEN** the packaged defaults or user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID or unsupported Windows version)
- **THEN** on Windows release desktop builds, the system displays a `MessageBoxW` dialog with the full actionable error message and exits with code 1; on all other platforms and build configurations, the system prints the actionable error message to stderr and exits with code 1

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state.

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, detects and cleans up any stale FUSE mount at the target path, creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list, and sends a "Mount Ready" notification

#### Scenario: Start mount failure — root resolution
- **WHEN** the system attempts to start a mount but the drive root item cannot be fetched from the Graph API
- **THEN** the mount is skipped, an error is logged with the drive name and reason, a system notification titled "Mount Failed" is sent with the mount name and error description, and other mounts continue unaffected

#### Scenario: Start mount — stale FUSE mount detected
- **WHEN** the system attempts to create or access the mount point directory and the path is a stale FUSE mount (stat returns ENOTCONN or EIO)
- **THEN** the system attempts to clean up the stale mount via `fusermount -u` (or `umount` on macOS), logs the cleanup result, and retries directory creation; if cleanup fails, the mount is skipped, a system notification titled "Mount Failed" is sent with the mount name and the remediation command (`fusermount -u <path>`), and an error is logged with the same actionable message

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or CfApi session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id, root resolution failure, unrecoverable stale mount); for each skipped mount, the system logs the reason and sends a "Mount Failed" notification with the mount name and error description

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system stops all active mounts, clears authentication tokens from secure storage, removes account metadata from user config, saves the config, reloads the wizard window to step-welcome (if the window exists), reloads the settings window to a clean DOM state (if the window exists), and transitions to the unauthenticated tray state (showing "Sign In…" in the tray menu and the wizard window)

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts
