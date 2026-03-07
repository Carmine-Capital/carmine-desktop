## MODIFIED Requirements

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state.

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, detects and cleans up any stale FUSE mount at the target path, creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list, and sends a "Mount Ready" notification

#### Scenario: Start mount failure — root resolution
- **WHEN** the system attempts to start a mount but the drive root item cannot be fetched from the Graph API
- **THEN** the mount is skipped, an error is logged with the drive name and reason, no notification is sent, and other mounts continue unaffected

#### Scenario: Start mount — stale FUSE mount detected
- **WHEN** the system attempts to create or access the mount point directory and the path is a stale FUSE mount (stat returns ENOTCONN or EIO)
- **THEN** the system attempts to clean up the stale mount via `fusermount -u` (or `umount` on macOS), logs the cleanup result, and retries directory creation; if cleanup fails, the mount is skipped with an actionable error message suggesting manual `fusermount -u <path>`

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or CfApi session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id, root resolution failure, unrecoverable stale mount), and logs skipped mounts with the reason

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system stops all active mounts, clears authentication tokens from secure storage, removes account metadata from user config, and reverts to the unauthenticated state (showing the wizard on next interaction)

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts

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
