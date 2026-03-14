## Purpose

Embed the WinFsp MSI installer into the Carmine Desktop MSI package and silently install it during setup if WinFsp is not already present on the system.

## Requirements

### Requirement: MSI installer bundles WinFsp
The installer SHALL use the MSI (WiX) format instead of NSIS, and check the Windows registry for an existing WinFsp installation before attempting to install it.

#### Scenario: WinFsp already installed
- **WHEN** the MSI installer runs and the registry key `HKLM\SOFTWARE\WinFsp` contains an `InstallDir` value
- **THEN** the installer skips WinFsp installation and proceeds with Carmine Desktop installation

#### Scenario: WinFsp not installed
- **WHEN** the MSI installer runs and the registry key `HKLM\SOFTWARE\WinFsp` does not exist or has no `InstallDir` value
- **THEN** the installer extracts the embedded WinFsp MSI to a temporary directory and proceeds to install it

### Requirement: Silent WinFsp MSI installation
The installer SHALL install WinFsp silently with full feature set when WinFsp is not present.

#### Scenario: Successful silent install
- **WHEN** the installer executes `msiexec /i "$TEMP\winfsp.msi" /qn INSTALLLEVEL=1000`
- **THEN** WinFsp is installed with all features (including the kernel driver), the temporary MSI file is deleted, and Carmine Desktop installation continues

#### Scenario: WinFsp install fails
- **WHEN** the `msiexec` command returns a non-zero exit code
- **THEN** the installer displays a message box stating that WinFsp installation failed and that Carmine Desktop requires WinFsp, then aborts the installation

### Requirement: Temporary file cleanup
The installer SHALL remove the extracted WinFsp MSI from the temporary directory after installation completes or fails.

#### Scenario: Cleanup after successful install
- **WHEN** the WinFsp MSI installation succeeds
- **THEN** the file `$TEMP\winfsp.msi` is deleted before continuing

#### Scenario: Cleanup after failed install
- **WHEN** the WinFsp MSI installation fails
- **THEN** the file `$TEMP\winfsp.msi` is deleted before aborting

### Requirement: No WinFsp removal on uninstall
The installer SHALL NOT remove WinFsp when Carmine Desktop is uninstalled.

#### Scenario: Carmine Desktop uninstall leaves WinFsp intact
- **WHEN** the user uninstalls Carmine Desktop
- **THEN** WinFsp remains installed on the system

### Requirement: Tauri bundle target configuration
The Tauri build configuration SHALL specify MSI as the Windows bundle target.

#### Scenario: Bundle target set to MSI
- **WHEN** the Tauri build runs on Windows
- **THEN** `tauri.conf.json` specifies `"targets": ["msi"]` under `bundle.windows` with WiX configuration `"language": "en-US"`

### Requirement: WinFsp copyright attribution
The application SHALL display the WinFsp copyright notice and repository link in the user interface to satisfy the redistribution license requirement.

#### Scenario: Attribution visible in settings
- **WHEN** the user views the settings page
- **THEN** the page displays "WinFsp - Windows File System Proxy, Copyright (C) Bill Zissimopoulos" with a link to the WinFsp GitHub repository
