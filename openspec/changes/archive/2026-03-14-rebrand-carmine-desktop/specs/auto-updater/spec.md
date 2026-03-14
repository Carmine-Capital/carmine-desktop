## MODIFIED Requirements

### Requirement: Update check on startup
The system SHALL check for available updates shortly after application startup in desktop mode.

#### Scenario: Default endpoint for release builds
- **WHEN** the application is built with the release pipeline and the updater endpoint is configured in `tauri.conf.json`
- **THEN** the endpoint points to `https://static.carminecapital.com/carmine-desktop/latest.json`

#### Scenario: Startup update check
- **WHEN** the application starts in desktop mode and the updater endpoint is configured (non-empty)
- **THEN** the system waits 10 seconds after initialization completes, then checks the configured endpoint for a newer version

#### Scenario: No updater endpoint configured
- **WHEN** the application starts and the updater endpoint list is empty (generic/dev build)
- **THEN** the system skips all update checks and does not register the periodic update timer

#### Scenario: Startup check finds update
- **WHEN** the startup update check finds a newer version available
- **THEN** the system downloads the update in the background and sends a notification "{app_name} v{version} is ready — restart to update"

#### Scenario: Startup check finds no update
- **WHEN** the startup update check finds the current version is up to date
- **THEN** the system logs "Up to date (v{current_version})" at debug level and takes no further action

#### Scenario: Startup check fails
- **WHEN** the startup update check fails (network error, invalid response, endpoint unreachable)
- **THEN** the system logs the error at warn level and continues normal operation; the periodic check will retry later
