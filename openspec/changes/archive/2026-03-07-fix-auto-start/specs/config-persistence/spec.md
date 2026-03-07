## ADDED Requirements

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
