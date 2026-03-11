## MODIFIED Requirements

### Requirement: Mount lifecycle management
The system SHALL manage the lifecycle of filesystem mounts — starting, stopping, and restarting them based on configuration and authentication state. The `start_mount` function SHALL extract shared initialization logic (drive validation, cache directory resolution, CacheManager creation, InodeTable setup, event channel wiring, state insertion) into a platform-agnostic helper, with only the platform-specific mount handle construction remaining in cfg-gated code. On Windows, the `account_name` parameter passed to `CfMountHandle::mount()` SHALL be the mount configuration's display name (not the Graph API drive ID). 

`start_mount` SHALL NOT send notifications — the caller is responsible for notification dispatch. This enables different notification strategies: batch summaries for startup (`start_all_mounts`) and per-mount notifications for user-initiated actions (`add_mount`, `toggle_mount`).

#### Scenario: Start mount
- **WHEN** the system needs to mount a drive (after sign-in, on startup with valid tokens, or when a new mount is added)
- **THEN** it resolves the drive root item from the Graph API, detects and cleans up any stale FUSE mount at the target path, creates the mount point directory if it does not exist, starts a FUSE or CfApi session for the drive with the root inode pre-seeded, adds the drive to the delta sync timer's drive list
- **AND** the function returns success without sending any notification

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
