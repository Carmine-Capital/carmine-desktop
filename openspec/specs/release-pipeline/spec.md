## Purpose

Provide a CI workflow that builds, signs (updater-level), and publishes platform-specific packages to GitHub Releases when a version tag is pushed.

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
- **THEN** the system produces an NSIS installer (`.exe`) for x86_64, with a corresponding `.sig` updater signature file

### Requirement: Updater signature generation
The system SHALL sign all release bundles with the Tauri ed25519 updater key during the build.

#### Scenario: Signing with updater key
- **WHEN** the build step runs and `TAURI_SIGNING_PRIVATE_KEY` is available as a secret
- **THEN** each produced bundle has a `.sig` file containing the ed25519 signature

#### Scenario: Missing signing key
- **WHEN** the build step runs and `TAURI_SIGNING_PRIVATE_KEY` is not set
- **THEN** the build fails with an error indicating the signing key is missing

### Requirement: Draft-then-publish release pattern
The system SHALL use a draft release during builds, only publishing after all platform builds succeed.

#### Scenario: All builds succeed
- **WHEN** all platform build jobs complete successfully
- **THEN** the draft release is published (un-drafted) and becomes visible to users with all artifacts and `latest.json` attached

#### Scenario: Any build fails
- **WHEN** one or more platform build jobs fail
- **THEN** the release remains in draft state and is not visible to users

### Requirement: Updater manifest generation
The system SHALL produce a `latest.json` manifest compatible with `tauri-plugin-updater` and upload it to the GitHub Release.

#### Scenario: latest.json content
- **WHEN** all platform builds complete and upload their artifacts
- **THEN** the release contains a `latest.json` file with the version, platform-specific download URLs, and ed25519 signatures for each platform
