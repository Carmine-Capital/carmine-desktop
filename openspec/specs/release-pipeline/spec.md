## Purpose

Provide a CI workflow that builds, signs (updater-level), and publishes platform-specific packages to a private static server when a version tag is pushed.

## Requirements

### Requirement: Release workflow triggers on version tags
The system SHALL run the release workflow when a git tag matching `v*` is pushed to the repository.

#### Scenario: Tag push triggers release
- **WHEN** a tag matching the pattern `v*` (e.g., `v0.1.0`, `v1.0.0-beta.1`) is pushed
- **THEN** the release workflow starts and builds packages for all configured platforms

#### Scenario: Non-tag push does not trigger release
- **WHEN** a commit is pushed to any branch without a tag
- **THEN** the release workflow does not run

### Requirement: Version-tag consistency check
The system SHALL verify that the git tag version matches the version in `tauri.conf.json` before building.

#### Scenario: Tag matches configured version
- **WHEN** the tag `v0.2.0` is pushed and `tauri.conf.json` contains `"version": "0.2.0"`
- **THEN** the build proceeds normally

#### Scenario: Tag does not match configured version
- **WHEN** the tag `v0.3.0` is pushed but `tauri.conf.json` contains `"version": "0.2.0"`
- **THEN** the workflow fails immediately with an error indicating the version mismatch

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

### Requirement: Updater signature generation
The system SHALL sign all release bundles with the Tauri ed25519 updater key during the build.

#### Scenario: Signing with updater key
- **WHEN** the build step runs and `TAURI_SIGNING_PRIVATE_KEY` is available as a secret
- **THEN** each produced bundle has a `.sig` file containing the ed25519 signature

#### Scenario: Missing signing key
- **WHEN** the build step runs and `TAURI_SIGNING_PRIVATE_KEY` is not set
- **THEN** the build fails with an error indicating the signing key is missing

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
