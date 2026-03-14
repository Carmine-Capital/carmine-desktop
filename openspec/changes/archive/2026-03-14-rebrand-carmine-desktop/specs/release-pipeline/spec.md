## MODIFIED Requirements

### Requirement: Multi-platform package builds
The system SHALL build platform-specific packages for Linux, macOS, and Windows in parallel.

#### Scenario: Linux packages
- **WHEN** the release workflow runs the Linux build
- **THEN** the system produces a `.deb` package (x86_64) and an `.AppImage` (x86_64), each with a corresponding `.sig` updater signature file

#### Scenario: macOS packages
- **WHEN** the release workflow runs the macOS builds
- **THEN** the system produces a `.dmg` disk image for `aarch64` (Apple Silicon) and a `.dmg` for `x86_64` (Intel), each with a corresponding `.sig` updater signature file

#### Scenario: Windows packages
- **WHEN** the release workflow runs the Windows build
- **THEN** the system produces an MSI installer (`.msi`) for x86_64 using WiX, with a corresponding `.sig` updater signature file

### Requirement: Draft-then-publish release pattern
The system SHALL upload signed artifacts and the updater manifest to the private static server via rsync over SSH.

#### Scenario: All builds succeed
- **WHEN** all platform build jobs complete successfully
- **THEN** the publish job generates `latest.json` from the build artifacts, then uploads all artifacts and `latest.json` to `static.carminecapital.com:/var/www/static/carmine-desktop/` via rsync over SSH

#### Scenario: Any build fails
- **WHEN** one or more platform build jobs fail
- **THEN** the publish job does not run and no artifacts are uploaded to the server

#### Scenario: SSH key not configured
- **WHEN** the publish job runs but the `DEPLOY_SSH_KEY` GitHub secret is not set
- **THEN** the rsync step fails with a clear error indicating the missing SSH key

### Requirement: Updater manifest generation
The system SHALL produce a `latest.json` manifest compatible with `tauri-plugin-updater` and upload it to the private static server.

#### Scenario: latest.json content
- **WHEN** all platform builds complete and the publish job runs
- **THEN** the publish job generates a `latest.json` file with the version, platform-specific download URLs pointing to `https://static.carminecapital.com/carmine-desktop/`, and ed25519 signatures for each platform

### Requirement: WinFsp MSI download for Windows builds
The release and build-installer workflows SHALL download a pinned WinFsp MSI from GitHub releases during Windows build jobs.

#### Scenario: Windows build downloads WinFsp MSI
- **WHEN** the workflow runs a Windows build job
- **THEN** the workflow downloads the WinFsp MSI specified by the pinned version variable to `crates/carminedesktop-app/resources/winfsp.msi`

#### Scenario: WinFsp MSI download fails
- **WHEN** the WinFsp MSI download fails (network error, 404, etc.)
- **THEN** the workflow fails with a clear error message indicating the download URL and failure reason

#### Scenario: Non-Windows builds skip WinFsp MSI download
- **WHEN** the workflow runs a Linux or macOS build job
- **THEN** no WinFsp MSI download step executes
