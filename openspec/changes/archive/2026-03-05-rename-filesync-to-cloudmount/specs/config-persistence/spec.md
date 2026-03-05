## MODIFIED Requirements

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
