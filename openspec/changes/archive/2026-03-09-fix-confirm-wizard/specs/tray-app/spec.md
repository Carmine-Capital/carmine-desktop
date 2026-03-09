## MODIFIED Requirements

### Requirement: Settings window
The system SHALL provide a settings window accessible from the tray menu. The Mounts tab SHALL allow the user to add new mounts by opening the wizard window. The settings window SHALL always display current persisted state when opened or re-shown — it SHALL NOT display unsaved form values from a previous session or account information from before a sign-out.

#### Scenario: Open settings
- **WHEN** the user selects "Settings..." from the tray menu
- **THEN** a settings window opens with tabs: General, Mounts, Account, Advanced

#### Scenario: General settings
- **WHEN** the user views the General tab
- **THEN** they can configure: auto-start on login (toggle), notification preferences (toggle), and global sync interval (dropdown: 30s, 1m, 5m, 15m)

#### Scenario: Enabling auto-start registers with OS
- **WHEN** the user enables the "Start on login" toggle and saves settings
- **THEN** the application registers itself with the OS login mechanism (systemd user service on Linux, LaunchAgent on macOS, Run registry key on Windows) so that the application launches automatically after the next login

#### Scenario: Disabling auto-start deregisters from OS
- **WHEN** the user disables the "Start on login" toggle and saves settings
- **THEN** the application removes its OS login entry so that it no longer launches automatically after the next login

#### Scenario: Auto-start registration failure is reported to the user
- **WHEN** the OS registration or deregistration call fails after saving the auto-start toggle
- **THEN** a system notification is displayed to the user indicating that auto-start registration failed and showing a brief reason, and the failure is logged as a warning; the setting is still persisted to the configuration file

#### Scenario: Mount settings
- **WHEN** the user views the Mounts tab
- **THEN** they see a list of all configured mounts with controls to enable/disable, change mount point, remove, and add new mounts

#### Scenario: Add Mount from Settings opens wizard
- **WHEN** the user clicks "Add Mount" in the Mounts tab of the Settings window
- **THEN** the wizard window is opened (or focused if already open) and navigated to `step-sources` so the user can add a SharePoint or OneDrive mount; the settings window remains open in the background

#### Scenario: Remove mount confirmation
- **WHEN** the user clicks "Remove" for a mount in the Mounts tab
- **THEN** the system SHALL display a native OS confirmation dialog via the Tauri dialog plugin before proceeding; if the user confirms, the mount is removed from the configuration and unmounted; if the user cancels, no action is taken

#### Scenario: Account tab — signed in
- **WHEN** the user views the Account tab and is authenticated
- **THEN** they see the account display name (or email if available) and a "Sign Out" button

#### Scenario: Account tab — not signed in
- **WHEN** the user views the Account tab and is NOT authenticated
- **THEN** they see "Not signed in" text and no "Sign Out" button (or a disabled one)

#### Scenario: Sign out from Account tab
- **WHEN** the user clicks "Sign Out" in the Account tab
- **THEN** the system SHALL display a native OS confirmation dialog via the Tauri dialog plugin ("Sign out? All mounts will stop."); if the user confirms, the system performs sign-out (stops mounts, clears tokens, updates config), reloads the settings window to a clean DOM state, and opens the wizard at step-welcome; if the user cancels, no action is taken

#### Scenario: Advanced settings
- **WHEN** the user views the Advanced tab
- **THEN** they can configure: cache directory path, maximum cache size, metadata TTL, debug logging toggle, and a "Clear Cache" button

#### Scenario: Settings window refreshed on re-show
- **WHEN** the settings window already exists (was previously shown and hidden) and is re-shown by any mechanism (tray menu, tray icon left-click)
- **THEN** the system SHALL reload the current settings and mount list from the backend before the window becomes visible, so the user always sees persisted state and never unsaved form values from a prior session

#### Scenario: Settings window reloaded after sign-out
- **WHEN** the user signs out (via tray menu or Account tab "Sign Out" button)
- **THEN** the settings window SHALL be reloaded (full page reload, not merely hidden) so that any subsequent open of the settings window starts from a clean DOM with no residual account display name, mount list, or tab state from the pre-sign-out session
