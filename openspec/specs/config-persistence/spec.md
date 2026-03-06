### Requirement: Configuration file format
The system SHALL store user-specific configuration (overrides and additions) in a TOML file at the platform-appropriate configuration directory. This file represents the user layer in the four-layer config resolution (CLI args > env vars > user config > packaged defaults).

#### Scenario: Configuration file location
- **WHEN** the application reads or writes configuration
- **THEN** it uses the path specified by `--config` or `CLOUDMOUNT_CONFIG` if provided, otherwise `~/.config/cloudmount/config.toml` on Linux, `~/Library/Application Support/cloudmount/config.toml` on macOS, and `%APPDATA%\CloudMount\config.toml` on Windows

#### Scenario: Configuration created on first run
- **WHEN** the application starts and no configuration file exists at the resolved path
- **THEN** the system creates the configuration directory and an empty user config file (the effective config is fully derived from packaged defaults if present, or built-in defaults otherwise)

#### Scenario: Configuration file is human-readable
- **WHEN** a user opens the configuration file in a text editor
- **THEN** the TOML format is readable with comments explaining each section and option

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
