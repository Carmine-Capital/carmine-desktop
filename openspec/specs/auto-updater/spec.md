## ADDED Requirements

### Requirement: Update check on startup
The system SHALL check for available updates shortly after application startup in desktop mode.

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

### Requirement: Periodic update checks
The system SHALL periodically check for updates while running in desktop mode.

#### Scenario: Periodic check interval
- **WHEN** the application is running in desktop mode with a configured updater endpoint
- **THEN** the system checks for updates every 4 hours after the initial startup check

#### Scenario: Periodic check finds update
- **WHEN** a periodic update check finds a newer version and no update has been downloaded yet
- **THEN** the system downloads the update in the background and sends a notification "{app_name} v{version} is ready — restart to update"

#### Scenario: Update already downloaded
- **WHEN** an update check runs but an update has already been downloaded and is pending installation
- **THEN** the system skips the download and takes no additional action

### Requirement: Manual update check
The system SHALL allow the user to manually trigger an update check from the tray menu.

#### Scenario: Manual check via tray menu
- **WHEN** the user selects "Check for Updates" from the tray context menu
- **THEN** the system immediately checks the configured endpoint for a newer version

#### Scenario: Manual check finds update
- **WHEN** a manual update check finds a newer version
- **THEN** the system downloads the update and sends a notification "{app_name} v{version} is ready — restart to update"

#### Scenario: Manual check finds no update
- **WHEN** a manual update check finds the current version is up to date
- **THEN** the system sends a notification "{app_name} is up to date"

#### Scenario: Manual check with no endpoint configured
- **WHEN** the user selects "Check for Updates" and no updater endpoint is configured
- **THEN** the system sends a notification "Update checking is not configured for this build"

### Requirement: Update download
The system SHALL download updates in the background without disrupting filesystem operations.

#### Scenario: Background download
- **WHEN** an update is available and download begins
- **THEN** the download runs in a background task; all filesystem operations (reads, writes, sync) continue unaffected

#### Scenario: Download progress
- **WHEN** an update is being downloaded
- **THEN** the system logs download progress at debug level (bytes downloaded / total bytes)

#### Scenario: Download failure
- **WHEN** an update download fails (network error, disk space, interrupted)
- **THEN** the system logs the error at warn level and retries on the next periodic check cycle

### Requirement: Update signature verification
The system SHALL verify the ed25519 signature of downloaded updates before installation.

#### Scenario: Valid signature
- **WHEN** an update bundle is downloaded and its signature matches the public key embedded in the application
- **THEN** the system marks the update as ready for installation

#### Scenario: Invalid signature
- **WHEN** an update bundle is downloaded but its signature does not match the embedded public key
- **THEN** the system rejects the update, logs an error "Update signature verification failed", and does not install it

#### Scenario: Missing signature
- **WHEN** an update bundle is downloaded but no signature file is present
- **THEN** the system rejects the update and logs an error "Update signature missing"

### Requirement: Update installation on restart
The system SHALL install downloaded updates when the user restarts the application.

#### Scenario: Restart to update via tray menu
- **WHEN** an update is downloaded and pending, and the user selects "Restart to Update" from the tray menu
- **THEN** the system performs the standard graceful shutdown (flush pending writes, unmount all drives, stop sync), installs the update, and relaunches the application

#### Scenario: Graceful shutdown before update
- **WHEN** the user triggers "Restart to Update"
- **THEN** the system SHALL complete the full graceful shutdown sequence (including the 30-second pending write flush timeout) before proceeding with update installation

#### Scenario: Update pending indicator in tray
- **WHEN** an update has been downloaded and is pending installation
- **THEN** the tray context menu SHALL show "Restart to Update (v{version})" in place of "Check for Updates"

### Requirement: Updater is desktop-only
The system SHALL only perform update checks in desktop mode. Headless mode SHALL NOT include update functionality.

#### Scenario: Headless mode
- **WHEN** the application is running in headless mode (no Tauri runtime)
- **THEN** no update checks are performed, no update-related code is initialized

#### Scenario: Desktop feature gate
- **WHEN** the application is compiled without the `desktop` feature flag
- **THEN** the update module is not compiled into the binary
