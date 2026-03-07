## MODIFIED Requirements

### Requirement: General settings
The system SHALL provide a General settings tab where the user can configure application-wide preferences. Changes to auto-start MUST apply the OS-level registration immediately upon save, not merely persist the preference.

#### Scenario: General settings
- **WHEN** the user views the General tab
- **THEN** they can configure: auto-start on login (toggle), notification preferences (toggle), and global sync interval (dropdown: 30s, 1m, 5m, 15m)

#### Scenario: Enabling auto-start registers with OS
- **WHEN** the user enables the "Start on login" toggle and saves settings
- **THEN** the application registers itself with the OS login mechanism (systemd user service on Linux, LaunchAgent on macOS, Run registry key on Windows) so that the application launches automatically after the next login

#### Scenario: Disabling auto-start deregisters from OS
- **WHEN** the user disables the "Start on login" toggle and saves settings
- **THEN** the application removes its OS login entry so that it no longer launches automatically after the next login

#### Scenario: Auto-start registration failure is reported to the user
- **WHEN** the OS registration or deregistration call fails after saving the auto-start toggle
- **THEN** a system notification is displayed to the user indicating that auto-start registration failed and showing a brief reason, and the failure is logged as a warning; the setting is still persisted to the configuration file

## ADDED Requirements

### Requirement: Auto-start failure notification
The system SHALL notify the user when auto-start registration or deregistration fails so they are aware the OS-level setting did not take effect.

#### Scenario: Notification content on failure
- **WHEN** the `autostart::set_enabled()` call returns an error
- **THEN** a desktop notification is displayed with a title of "Auto-start" and a body describing the failure (e.g., "Failed to register auto-start: systemctl not found"), using the same notification delivery mechanism as other application notifications
