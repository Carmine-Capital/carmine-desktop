## MODIFIED Requirements

### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, each CfApi mount SHALL use a unique sync root ID by including an `account_name` discriminator in the sync root ID construction. The sync root ID format SHALL be `<provider>!<security-id>!<account_name>`. The `account_name` parameter SHALL be required when calling `CfMountHandle::mount()`. The `account_name` value MUST NOT contain `!` (exclamation mark) characters, as `!` is the sync root ID component separator. When constructing the account_name from a Microsoft Graph drive ID, the caller SHALL replace all `!` characters with `_` before passing it to the mount function.

On Windows, `CfMountHandle::mount()` SHALL accept a `display_name` parameter separate from `account_name`. The sync root SHALL be registered with `display_name` as the user-visible label shown in File Explorer's navigation pane. The `display_name` SHALL be the user-visible mount name (e.g., the value of `mount_config.name`) without `!`-sanitization. The sync root SHALL be registered unconditionally on every mount call (not only when previously unregistered) so that stale display names from prior launches are corrected.

On Windows, `CfMountHandle::mount()` SHALL spawn a filesystem watcher thread and a periodic timer thread as part of mount initialization. The watcher thread SHALL monitor the sync root for local file changes. The timer thread SHALL process deferred operations on a 500ms interval. Both threads SHALL terminate when the mount is stopped.

#### Scenario: Windows mount initialization starts watcher and timer
- **WHEN** a CfApi sync root is mounted on Windows
- **THEN** the system spawns the filesystem watcher thread and the periodic timer thread before returning from `mount()`
- **AND** both threads hold `Arc` references to the sync filter and terminate cleanly on unmount

#### Scenario: Writeback stages local-only file
- **WHEN** `stage_writeback_from_disk()` encounters a file whose item ID starts with `local:`
- **THEN** the system skips the mtime/size unmodified comparison and always proceeds to stage the file for upload
- **AND** the file content is read and queued for `flush_inode()`

#### Scenario: Writeback skips unmodified server-backed file
- **WHEN** `stage_writeback_from_disk()` encounters a file whose item ID does not start with `local:` and the file's mtime matches the cached DriveItem within 1 second and the file size equals the cached size
- **THEN** the system skips the file as unmodified and does not stage it for upload

#### Scenario: Rename callback acknowledges on failure
- **WHEN** the `rename()` CfApi callback fires and `core.rename()` fails with a Graph API error
- **THEN** the system calls `ticket.pass()` to acknowledge the rename to the OS and logs a warning about the Graph API failure
- **AND** the local rename is preserved (the OS has already performed it on disk) and the Graph API rename will be retried by subsequent sync operations

#### Scenario: Rename callback acknowledges on success
- **WHEN** the `rename()` CfApi callback fires and `core.rename()` succeeds
- **THEN** the system calls `ticket.pass()` to acknowledge the rename to the OS
