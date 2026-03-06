## MODIFIED Requirements

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state.

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list, and sends a "Mount Ready" notification

#### Scenario: Start mount failure — root resolution
- **WHEN** the system attempts to start a mount but the drive root item cannot be fetched from the Graph API
- **THEN** the mount is skipped, an error is logged with the drive name and reason, no notification is sent, and other mounts continue unaffected

#### Scenario: Stop mount
- **WHEN** the system needs to unmount a drive (on sign-out, mount removal, or application quit)
- **THEN** it flushes all pending writes for the drive (30-second timeout), unmounts the FUSE or CfApi session, and removes the drive from the delta sync timer's drive list

#### Scenario: Start all mounts after authentication
- **WHEN** the user successfully authenticates or tokens are restored on startup
- **THEN** the system starts mounts for all enabled mount configurations in order, skipping any with errors (invalid mount point, missing drive_id, root resolution failure), and logs skipped mounts with the reason

#### Scenario: Stop all mounts on sign-out
- **WHEN** the user signs out
- **THEN** the system stops all active mounts, clears authentication tokens from secure storage, removes account metadata from user config, and reverts to the unauthenticated state (showing the wizard on next interaction)

#### Scenario: Mount config change
- **WHEN** the user adds, removes, toggles, or changes the mount point of a mount in settings
- **THEN** the system applies the change immediately — starting, stopping, or restarting the affected mount — without affecting other active mounts
