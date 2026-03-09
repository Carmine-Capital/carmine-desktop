## MODIFIED Requirements

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

#### Scenario: Default endpoint for release builds
- **WHEN** the application is built with the release pipeline and the updater endpoint is configured in `tauri.conf.json`
- **THEN** the endpoint points to `https://github.com/{owner}/{repo}/releases/latest/download/latest.json`

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

#### Scenario: Public key embedded in build
- **WHEN** the application is built via the release pipeline
- **THEN** the ed25519 public key is embedded in `tauri.conf.json` under `plugins.updater.pubkey` and used to verify all update signatures
