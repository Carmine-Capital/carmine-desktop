## Purpose
Defines user configuration persistence, hot-reload, auto-start, and validation for carminedesktop.

## Requirements

### Requirement: Configuration file format
The system SHALL store user-specific configuration in a TOML file at the platform-appropriate configuration directory. This file represents the user layer in a two-layer config resolution: CLI args and env vars take precedence over user config, which in turn takes precedence over built-in application defaults. There is no packaged defaults layer.

#### Scenario: Configuration file location
- **WHEN** the application reads or writes configuration
- **THEN** it uses the path specified by `--config` or `carminedesktop_CONFIG` if provided, otherwise `~/.config/carminedesktop/config.toml` on Linux, `~/Library/Application Support/carminedesktop/config.toml` on macOS, and `%APPDATA%\carminedesktop\config.toml` on Windows

#### Scenario: Configuration created on first run
- **WHEN** the application starts and no configuration file exists at the resolved path
- **THEN** the system creates the configuration directory and writes an empty user config file; the effective config is fully derived from built-in application defaults

#### Scenario: Configuration file is human-readable
- **WHEN** a user opens the configuration file in a text editor
- **THEN** the TOML format is readable with comments explaining each section and option

### Requirement: Root mount directory setting
The system SHALL provide a configurable root directory name under which all auto-created mount points are placed.

#### Scenario: Default root directory
- **WHEN** no root_dir is configured in user config
- **THEN** the system uses "Cloud" as the default root directory name, resulting in mount points under `~/Cloud/`

#### Scenario: Custom root directory from user config
- **WHEN** the user sets `root_dir = "MyDrives"` in the `[general]` section of their config
- **THEN** new auto-created mounts use `~/MyDrives/` as their root (e.g., `~/MyDrives/OneDrive/`)

#### Scenario: OneDrive mount point derivation
- **WHEN** the system auto-creates an OneDrive mount
- **THEN** the mount point is set to `{home}/{root_dir}/OneDrive`

#### Scenario: SharePoint mount point derivation
- **WHEN** the user adds a SharePoint document library mount
- **THEN** the mount point is set to `{home}/{root_dir}/{SiteName} - {LibraryName}`

#### Scenario: Root directory does not affect existing mounts
- **WHEN** the user changes the root_dir setting after mounts have already been created
- **THEN** existing mount points are NOT retroactively changed; only newly created mounts use the updated root directory

### Requirement: Auto-start setting applies OS registration immediately
The system SHALL register or deregister the application with the OS login mechanism whenever the `auto_start` setting is saved, not merely persist the value to the configuration file.

#### Scenario: Enable auto-start registers OS entry
- **WHEN** the user saves settings with `auto_start = true`
- **THEN** the system writes the platform-appropriate OS launch entry (systemd user service on Linux, LaunchAgent plist on macOS, `HKCU\...\Run` registry key on Windows) before returning from the save operation

#### Scenario: Disable auto-start removes OS entry
- **WHEN** the user saves settings with `auto_start = false`
- **THEN** the system removes the platform-appropriate OS launch entry if it exists

#### Scenario: Auto-start registration failure is non-fatal
- **WHEN** writing the OS launch entry fails (e.g., `systemctl` not found, filesystem permission error)
- **THEN** the configuration file is still saved successfully, a warning is logged, and the user receives a notification explaining the failure; the save_settings command returns success

### Requirement: Auto-start OS state is reconciled on startup
The system SHALL reconcile the OS-level auto-start registration with the persisted `auto_start` configuration value each time the application launches.

#### Scenario: Config has auto-start enabled, OS entry is missing
- **WHEN** the application starts and `effective_config.auto_start` is `true` but no OS launch entry exists (e.g., it was manually removed, or this is the first run after upgrading)
- **THEN** the system re-creates the OS launch entry using the current executable path

#### Scenario: Config has auto-start disabled, OS entry exists
- **WHEN** the application starts and `effective_config.auto_start` is `false` but an OS launch entry exists (e.g., left over from a previous enable)
- **THEN** the system removes the OS launch entry

#### Scenario: Startup sync failure is non-fatal
- **WHEN** the startup reconciliation call fails (e.g., cannot resolve current executable path)
- **THEN** a warning is logged and application startup continues normally; mounts and sync are not affected

#### Scenario: Executable path is kept current
- **WHEN** the application is updated in-place and the executable path changes
- **THEN** the next application launch re-registers the OS entry with the updated executable path, ensuring the auto-start entry points to the correct binary

### Requirement: Mount point path normalization
The system SHALL normalize expanded mount point paths to use OS-native path separators and SHALL strip any trailing separator before the path is used for filesystem operations or passed to the virtual filesystem backend.

#### Scenario: Forward slashes normalized to backslashes on Windows
- **WHEN** a mount point template `~/Cloud/OneDrive` is expanded on Windows
- **THEN** the expanded path uses backslash separators throughout (e.g. `C:\Users\nyxa\Cloud\OneDrive`), with no forward slashes remaining

#### Scenario: Trailing separator stripped
- **WHEN** a mount point template has a trailing `/` or `\` (e.g. `~/Cloud/MyDrive/`)
- **THEN** the expanded path has no trailing separator (e.g. `~/Cloud/MyDrive` becomes `C:\Users\nyxa\Cloud\MyDrive` on Windows or `/home/nyxa/Cloud/MyDrive` on Linux)

#### Scenario: Bare drive root preserved on Windows
- **WHEN** a mount point expands to a bare drive root like `C:\`
- **THEN** the trailing backslash is preserved because `C:` without the separator changes meaning on Windows

#### Scenario: Defensive normalization at mount time
- **WHEN** a mount point path reaches the mount setup function
- **THEN** the system SHALL strip trailing separators as a safety net regardless of whether `expand_mount_point` was the source of the path

### Requirement: Mount point templates stored without trailing separators
The system SHALL strip trailing path separators from mount point values before persisting them to the configuration file.

#### Scenario: SharePoint mount created with trailing separator in input
- **WHEN** a SharePoint mount is added and the derived mount point template ends with `/` or `\`
- **THEN** the persisted mount point template in the configuration file has no trailing separator

#### Scenario: OneDrive mount created with clean template
- **WHEN** an OneDrive mount is added
- **THEN** the persisted mount point template has no trailing separator
