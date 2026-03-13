## ADDED Requirements

### Requirement: WinFsp MSI download for Windows builds
The release and build-installer workflows SHALL download a pinned WinFsp MSI from GitHub releases during Windows build jobs.

#### Scenario: Windows build downloads WinFsp MSI
- **WHEN** the workflow runs a Windows build job
- **THEN** the workflow downloads the WinFsp MSI specified by the pinned version variable to `crates/cloudmount-app/resources/winfsp.msi`

#### Scenario: WinFsp MSI download fails
- **WHEN** the WinFsp MSI download fails (network error, 404, etc.)
- **THEN** the workflow fails with a clear error message indicating the download URL and failure reason

#### Scenario: Non-Windows builds skip WinFsp MSI download
- **WHEN** the workflow runs a Linux or macOS build job
- **THEN** no WinFsp MSI download step executes
