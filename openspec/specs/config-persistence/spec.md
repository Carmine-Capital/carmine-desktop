### Requirement: Configuration file format
The system SHALL store user-specific configuration (overrides and additions) in a TOML file at the platform-appropriate configuration directory. This file represents the user layer in the two-layer config resolution (packaged defaults + user config).

#### Scenario: Configuration file location
- **WHEN** the application reads or writes configuration
- **THEN** it uses `~/.config/cloudmount/config.toml` on Linux, `~/Library/Application Support/cloudmount/config.toml` on macOS, and `%APPDATA%\CloudMount\config.toml` on Windows

#### Scenario: Configuration created on first run
- **WHEN** the application starts and no configuration file exists
- **THEN** the system creates the configuration directory and an empty user config file (the effective config is fully derived from packaged defaults if present, or built-in defaults otherwise)

#### Scenario: Configuration file is human-readable
- **WHEN** a user opens the configuration file in a text editor
- **THEN** the TOML format is readable with comments explaining each section and option

### Requirement: Mount configuration persistence
The system SHALL persist all mount definitions in the configuration file.

#### Scenario: Save OneDrive mount
- **WHEN** the user configures a OneDrive mount
- **THEN** the system saves a `[[mounts]]` entry with: name, type="onedrive", account_id, drive_id, mount_point, and enabled=true

#### Scenario: Save SharePoint mount
- **WHEN** the user configures a SharePoint mount
- **THEN** the system saves a `[[mounts]]` entry with: name, type="sharepoint", account_id, site_id, site_name, drive_id, library_name, mount_point, and enabled=true

#### Scenario: Remove mount configuration
- **WHEN** the user removes a mount from the settings
- **THEN** the system removes the corresponding `[[mounts]]` entry from the configuration file and cleans up associated cache data

### Requirement: Account metadata persistence
The system SHALL persist non-sensitive account metadata in the configuration file. Tokens MUST be stored in the OS keychain only.

#### Scenario: Save account info
- **WHEN** the user successfully authenticates
- **THEN** the system saves an `[[accounts]]` entry with: id (generated UUID), email, display_name, and tenant_id. Tokens are stored separately in the OS keychain keyed by account_id.

#### Scenario: Account info readable without secrets
- **WHEN** a user inspects the configuration file
- **THEN** they see account email and display name but no tokens, passwords, or secrets

### Requirement: Auto-start configuration
The system SHALL configure itself to start automatically on user login when the auto_start option is enabled.

#### Scenario: Enable auto-start on Linux
- **WHEN** auto_start is set to true on Linux
- **THEN** the system creates a systemd user service file at `~/.config/systemd/user/cloudmount.service` and enables it

#### Scenario: Enable auto-start on macOS
- **WHEN** auto_start is set to true on macOS
- **THEN** the system creates a LaunchAgent plist at `~/Library/LaunchAgents/com.cloudmount.agent.plist` with RunAtLoad=true

#### Scenario: Enable auto-start on Windows
- **WHEN** auto_start is set to true on Windows
- **THEN** the system creates a registry entry at `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` with the application path

#### Scenario: Disable auto-start
- **WHEN** auto_start is set to false
- **THEN** the system removes the platform-specific auto-start mechanism (systemd service, LaunchAgent, or registry entry)

### Requirement: Configuration hot-reload
The system SHALL detect and apply configuration changes without requiring a full restart.

#### Scenario: Mount point changed
- **WHEN** the user changes a mount point in settings and clicks Save
- **THEN** the system unmounts the drive from the old path, remounts at the new path, and updates the configuration file

#### Scenario: Cache settings changed
- **WHEN** the user changes cache_max_size or metadata_ttl in settings
- **THEN** the system applies the new values immediately (evicting cache entries if the new size is smaller) without restarting

#### Scenario: Sync interval changed
- **WHEN** the user changes the sync interval in settings
- **THEN** the system reschedules the delta sync timer to the new interval immediately

### Requirement: User config records only overrides
The system SHALL store only user-modified values in the user config file. Settings at their default (packaged or built-in) value MUST NOT be written to the user config.

#### Scenario: User changes a setting
- **WHEN** the user changes cache_max_size from the default 5GB to 10GB in settings
- **THEN** the user config file contains only `cache_max_size = "10GB"` under `[general]`, not the full set of general settings

#### Scenario: User resets a setting to default
- **WHEN** the user clicks "Reset to Default" for a setting
- **THEN** the key is removed from user config, and the effective value reverts to the packaged default

#### Scenario: Dismissed packaged mounts tracked in user config
- **WHEN** the user dismisses a packaged mount
- **THEN** the user config file contains a `dismissed_packaged_mounts = ["mount-id"]` array

### Requirement: Configuration validation
The system SHALL validate configuration on load and handle errors gracefully.

#### Scenario: Corrupted configuration file
- **WHEN** the configuration file cannot be parsed as valid TOML
- **THEN** the system backs up the corrupted file as `config.toml.bak`, creates a fresh default configuration, displays a notification "Configuration was reset due to corruption", and starts with defaults

#### Scenario: Invalid mount point in configuration
- **WHEN** a saved mount point path is invalid or inaccessible on startup
- **THEN** the system skips that mount, marks it as errored in the tray menu, and logs a warning with the specific path issue

### Requirement: Root mount directory setting
The system SHALL provide a configurable root directory name under which all auto-created mount points are placed.

#### Scenario: Default root directory
- **WHEN** no root_dir is configured in user config or packaged defaults
- **THEN** the system uses "Cloud" as the default root directory name, resulting in mount points under `~/Cloud/`

#### Scenario: Custom root directory from user config
- **WHEN** the user sets `root_dir = "MyDrives"` in the `[general]` section of their config
- **THEN** new auto-created mounts use `~/MyDrives/` as their root (e.g., `~/MyDrives/OneDrive/`)

#### Scenario: Root directory from packaged defaults
- **WHEN** the packaged defaults specify `root_dir = "Contoso"` in `[defaults]`
- **THEN** auto-created mounts use `~/Contoso/` as their root, unless overridden by user config

#### Scenario: OneDrive mount point derivation
- **WHEN** the system auto-creates an OneDrive mount
- **THEN** the mount point is set to `{home}/{root_dir}/OneDrive`

#### Scenario: SharePoint mount point derivation
- **WHEN** the user adds a SharePoint document library mount from settings
- **THEN** the mount point is set to `{home}/{root_dir}/{SiteName}/{LibraryName}`

#### Scenario: Root directory does not affect existing mounts
- **WHEN** the user changes the root_dir setting after mounts have already been created
- **THEN** existing mount points are NOT retroactively changed; only newly created mounts use the updated root directory
