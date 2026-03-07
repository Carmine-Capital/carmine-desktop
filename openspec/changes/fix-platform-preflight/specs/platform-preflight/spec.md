## ADDED Requirements

### Requirement: Windows fatal error dialog
On Windows release desktop builds, the system SHALL display a native `MessageBoxW` dialog containing the full error message before terminating, so that errors are visible to users who launch the application without a console.

#### Scenario: Missing client ID on Windows release build
- **WHEN** `preflight_checks` detects a placeholder client ID on a Windows release desktop build
- **THEN** the system displays a `MessageBoxW` dialog with the error message and install instructions, then exits with code 1; no `eprintln!` call is made (stderr is detached)

#### Scenario: CfApi version check fails on Windows release build
- **WHEN** `preflight_checks` detects that the Windows version is below build 16299 (Windows 10 1709) on a Windows desktop build
- **THEN** the system displays a `MessageBoxW` dialog stating that Cloud Files API requires Windows 10 version 1709 or later, then exits with code 1

#### Scenario: Fatal error on non-Windows or debug build
- **WHEN** `preflight_checks` returns an error on Linux, macOS, or a Windows debug build
- **THEN** the system prints the error to stderr via `eprintln!` and exits with code 1 (existing behavior preserved)

### Requirement: Windows Cloud Files API version guard
The system SHALL verify that the Windows version supports Cloud Files API before attempting any CfApi mount operation.

#### Scenario: Supported Windows version
- **WHEN** `preflight_checks` runs on Windows 10 build 16299 or later
- **THEN** the CfApi version check passes silently and startup continues normally

#### Scenario: Unsupported Windows version
- **WHEN** `preflight_checks` runs on a Windows version earlier than build 16299
- **THEN** the system displays a `MessageBoxW` dialog with an actionable message ("Cloud Files API requires Windows 10 version 1709 or later") and exits with code 1

### Requirement: FUSE unavailable notification
On Linux and macOS desktop builds, when FUSE is not available, the system SHALL surface a system notification with platform-specific install instructions after sign-in completes, rather than silently degrading.

#### Scenario: FUSE absent on Linux after sign-in
- **WHEN** authentication succeeds on a Linux desktop build (either fresh sign-in via wizard or token restore on startup) and `fusermount3` is not found before mounts start
- **THEN** the system sends a system notification with the title "FUSE Not Installed" and body "Filesystem mounts require FUSE. Run: sudo apt install fuse3 (Debian/Ubuntu) or equivalent for your distribution."

#### Scenario: macFUSE absent on macOS after sign-in
- **WHEN** authentication succeeds on a macOS desktop build (either fresh sign-in via wizard or token restore on startup) and `fusermount` is not found before mounts start
- **THEN** the system sends a system notification with the title "macFUSE Not Installed" and body "Filesystem mounts require macFUSE. Install it from https://github.com/osxfuse/osxfuse/releases."

#### Scenario: FUSE present
- **WHEN** authentication succeeds and FUSE is available
- **THEN** no FUSE notification is sent and startup proceeds normally

### Requirement: Mount failure notification
The system SHALL send a system notification when a mount fails to start, so that the user can take action without consulting logs.

#### Scenario: Stale FUSE mount cannot be cleaned up
- **WHEN** `start_mount` fails because a stale FUSE mount at the target path cannot be cleaned up automatically
- **THEN** the system sends a system notification with the mount name and the error message (which includes the `fusermount -u <path>` remediation command), in addition to logging the error

#### Scenario: Mount fails for any reason
- **WHEN** `start_mount` returns an error for any reason
- **THEN** the system sends a system notification titled "Mount Failed" with the mount name and error body, in addition to the existing `tracing::error!` log entry

#### Scenario: Mount succeeds
- **WHEN** `start_mount` succeeds
- **THEN** the existing "Mount Ready" notification is sent (no change)
