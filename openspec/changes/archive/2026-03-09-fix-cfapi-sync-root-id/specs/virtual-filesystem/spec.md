## MODIFIED Requirements

### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system. Before the filesystem session is exposed to the OS, the system SHALL resolve the drive root item from the Graph API, register it in the inode table as ROOT_INODE (1), and seed it into the memory and SQLite caches. If the root item cannot be resolved, the mount SHALL fail with an error.

On Windows, each CfApi mount SHALL use a unique sync root ID by including an `account_name` discriminator in the sync root ID construction. The sync root ID format SHALL be `<provider>!<security-id>!<account_name>`. The `account_name` parameter SHALL be required when calling `CfMountHandle::mount()`. The `account_name` value MUST NOT contain `!` (exclamation mark) characters, as `!` is the sync root ID component separator. When constructing the account_name from a Microsoft Graph drive ID, the caller SHALL replace all `!` characters with `_` before passing it to the mount function.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows with an `account_name` identifier
- **THEN** the system fetches the drive root item from the Graph API, seeds it into caches as inode 1, registers a Cloud Files API sync root with a unique sync root ID derived from the provider name, user security ID, and account name, populates the directory with placeholder files, and the sync root appears as a first-class entry in File Explorer's navigation pane with cloud sync status icons

#### Scenario: Mount on Windows with drive ID containing exclamation marks
- **WHEN** the user enables a mount on Windows and the drive ID contains `!` characters (e.g., SharePoint/OneDrive Business `b!...` format)
- **THEN** the system sanitizes the account_name by replacing all `!` with `_` before constructing the sync root ID, producing a valid 3-component ID (`provider!SID!account_name_without_bangs`)

#### Scenario: Multiple concurrent Windows mounts
- **WHEN** two or more drives are mounted simultaneously on Windows, each with a distinct `account_name`
- **THEN** each mount SHALL have its own independent sync root registration, and CfApi callbacks SHALL be dispatched to the correct filter for each mount path

#### Scenario: Root resolution failure
- **WHEN** the drive root item cannot be fetched from the Graph API at mount time (network error, invalid drive_id, auth error)
- **THEN** the mount fails and returns an error; the mount point directory is not registered with FUSE/CfApi, and the error is logged and surfaced to the caller
