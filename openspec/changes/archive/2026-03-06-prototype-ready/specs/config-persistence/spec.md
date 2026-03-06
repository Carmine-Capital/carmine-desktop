## MODIFIED Requirements

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
