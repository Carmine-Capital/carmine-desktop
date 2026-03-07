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
- **THEN** the system SHALL, in order: (1) attempt to stop all active mounts (best-effort, errors logged but not fatal), (2) attempt to clear authentication tokens from secure storage, remove account metadata from user config, and save the config (best-effort, errors logged), (3) regardless of any failures in steps 1–2, set the authenticated flag to false, rebuild the tray menu to the unauthenticated state, reload the settings window to clean DOM state, and show the sign-in wizard; if any step in phase 1–2 produced an error, the system SHALL emit a desktop notification describing the failure

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts
