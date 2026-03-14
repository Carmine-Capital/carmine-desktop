## ADDED Requirements

### Requirement: Navigation pane root node registration
The system SHALL register a persistent "Carmine Desktop" entry in Windows Explorer's left navigation pane as a delegate folder CLSID pointing to the cloud root directory. The registration SHALL use HKCU registry keys so that no administrator privileges are required.

#### Scenario: Root node appears in navigation pane
- **WHEN** the explorer_nav_pane setting is enabled and the system registers the navigation pane entry
- **THEN** a "Carmine Desktop" node appears in Windows Explorer's left navigation pane with the application icon, and expanding it shows the contents of the cloud root directory (e.g., `C:\Users\<user>\Cloud`)

#### Scenario: Registry structure for delegate folder
- **WHEN** the system registers the navigation pane entry
- **THEN** it creates entries in three registry locations: (1) the CLSID definition at `HKCU\Software\Classes\CLSID\{GUID}` with DefaultIcon, InProcServer32 delegating to shell32.dll, Instance\InitPropertyBag with TargetFolderPath and Attributes, ShellFolder attributes, and shell\open\command; (2) the desktop namespace pin at `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{GUID}`; (3) the desktop icon suppression at `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel\{GUID}`

#### Scenario: Explorer is notified after registration
- **WHEN** the system completes a registration or unregistration of the navigation pane entry
- **THEN** it calls `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None)` to force Explorer to refresh the navigation pane

### Requirement: Navigation pane root node displays application icon
The system SHALL set the DefaultIcon of the root CLSID to the Carmine Desktop application executable icon (index 0).

#### Scenario: Icon from current executable
- **WHEN** the system registers the navigation pane entry
- **THEN** the DefaultIcon registry value is set to `"<current_exe_path>,0"` using the resolved path of the running executable

### Requirement: App launch on root node click
The system SHALL register a `shell\open\command` on the root CLSID so that clicking the navigation pane entry launches Carmine Desktop when the application is not running.

#### Scenario: App not running, user clicks root node
- **WHEN** the user clicks the "Carmine Desktop" entry in the navigation pane and the application is not running
- **THEN** Windows Explorer launches the Carmine Desktop executable registered in the CLSID's shell\open\command

#### Scenario: App already running, user clicks root node
- **WHEN** the user clicks the "Carmine Desktop" entry and the application is already running
- **THEN** Explorer navigates into the delegate folder showing the cloud root directory contents (standard Explorer behavior for delegate folders)

### Requirement: Dynamic child visibility via filesystem
The system SHALL NOT register child CLSIDs in the registry. Individual mount directories SHALL appear and disappear as children of the root node naturally, because WinFsp creates and removes the actual directories under the cloud root path.

#### Scenario: Mount started — child appears
- **WHEN** a WinFsp mount is started and creates a directory under the cloud root (e.g., `~/Cloud/OneDrive`)
- **THEN** the directory appears as a child of the "Carmine Desktop" node in Explorer's navigation pane with a standard folder icon

#### Scenario: Mount stopped — child disappears
- **WHEN** a WinFsp mount is stopped and its directory is removed
- **THEN** the directory disappears from the "Carmine Desktop" node in Explorer's navigation pane

### Requirement: Navigation pane unregistration
The system SHALL provide a function to remove all navigation pane registry entries, restoring the system to the state before registration.

#### Scenario: Full unregistration
- **WHEN** the system unregisters the navigation pane entry
- **THEN** it removes the CLSID key tree from `HKCU\Software\Classes\CLSID\{GUID}`, the namespace entry from `HKCU\...\Desktop\NameSpace\{GUID}`, and the hide-icon entry from `HKCU\...\HideDesktopIcons\NewStartPanel\{GUID}`, then calls SHChangeNotify

#### Scenario: Partial cleanup is resilient
- **WHEN** the system unregisters and some registry keys are already missing (e.g., manually deleted)
- **THEN** the unregistration succeeds without error for the missing keys and removes any remaining keys

### Requirement: Navigation pane target path update
The system SHALL update the TargetFolderPath in the CLSID registry when the cloud root directory path changes (e.g., user changes root_dir setting).

#### Scenario: Root directory setting changed
- **WHEN** the user changes the root_dir setting and saves, and the navigation pane is registered
- **THEN** the system updates the `Instance\InitPropertyBag\TargetFolderPath` value to the new expanded cloud root path and calls SHChangeNotify

### Requirement: Navigation pane setting toggle
The system SHALL expose the navigation pane feature as a toggleable setting in the UI, defaulting to enabled on Windows.

#### Scenario: Setting enabled by default on Windows
- **WHEN** the application runs on Windows and no explorer_nav_pane value is set in user config
- **THEN** the effective config resolves explorer_nav_pane to true

#### Scenario: Setting disabled on non-Windows
- **WHEN** the application runs on Linux or macOS
- **THEN** the effective config resolves explorer_nav_pane to false regardless of user config value

#### Scenario: User disables via settings UI
- **WHEN** the user disables the explorer_nav_pane toggle in the settings UI and saves
- **THEN** the system unregisters the navigation pane entry and persists the setting

#### Scenario: User enables via settings UI
- **WHEN** the user enables the explorer_nav_pane toggle and saves
- **THEN** the system registers the navigation pane entry and persists the setting
