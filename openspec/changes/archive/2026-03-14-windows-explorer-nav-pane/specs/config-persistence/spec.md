## MODIFIED Requirements

### Requirement: Auto-start setting applies OS registration immediately
The system SHALL register or deregister the application with the OS login mechanism whenever the `auto_start` setting is saved, not merely persist the value to the configuration file. The system SHALL also apply navigation pane registration changes immediately when the `explorer_nav_pane` setting is saved.

#### Scenario: Enable auto-start registers OS entry
- **WHEN** the user saves settings with `auto_start = true`
- **THEN** the system writes the platform-appropriate OS launch entry (systemd user service on Linux, LaunchAgent plist on macOS, `HKCU\...\Run` registry key on Windows) before returning from the save operation

#### Scenario: Disable auto-start removes OS entry
- **WHEN** the user saves settings with `auto_start = false`
- **THEN** the system removes the platform-appropriate OS launch entry if it exists

#### Scenario: Auto-start registration failure is non-fatal
- **WHEN** writing the OS launch entry fails (e.g., `systemctl` not found, filesystem permission error)
- **THEN** the configuration file is still saved successfully, a warning is logged, and the user receives a notification explaining the failure; the save_settings command returns success

#### Scenario: Enable explorer_nav_pane registers navigation pane
- **WHEN** the user saves settings with `explorer_nav_pane = true` on Windows
- **THEN** the system registers the navigation pane delegate folder entry before returning from the save operation

#### Scenario: Disable explorer_nav_pane unregisters navigation pane
- **WHEN** the user saves settings with `explorer_nav_pane = false` on Windows
- **THEN** the system unregisters the navigation pane entry if it exists before returning from the save operation

#### Scenario: Navigation pane registration failure is non-fatal
- **WHEN** the navigation pane registration or unregistration fails during settings save
- **THEN** the configuration file is still saved successfully, a warning is logged, and the save_settings command returns success
