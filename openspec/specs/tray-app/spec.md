## Purpose
Provides system tray presence, context menu, first-run wizard, settings window, and notifications for the CloudMount desktop application.
## Requirements
### Requirement: System tray presence
The system SHALL run as a background application with a system tray icon on all supported platforms.

#### Scenario: Tray icon on startup
- **WHEN** the application starts
- **THEN** a system tray icon appears in the OS notification area (Windows taskbar, macOS menu bar, Linux system tray)

#### Scenario: Tray icon states
- **WHEN** all mounts are synced and healthy
- **THEN** the tray icon displays a green/normal state indicator
- **WHEN** any mount is actively syncing
- **THEN** the tray icon displays a syncing animation/indicator
- **WHEN** any mount has an error (auth failure, network, conflict)
- **THEN** the tray icon displays a warning/error indicator

### Requirement: Tray context menu
The system SHALL provide a context menu when the user right-clicks (or clicks on macOS) the tray icon.

#### Scenario: Context menu contents
- **WHEN** the user activates the tray icon context menu and is authenticated
- **THEN** the menu displays: the list of configured mounts with their status (mounted/unmounted/error), a separator, "Add Mount...", "Settings...", "Check for Updates", "Sign Out", and "Quit"

#### Scenario: Context menu contents — unauthenticated
- **WHEN** the user activates the tray icon context menu and is NOT authenticated
- **THEN** the menu displays: "Settings...", "Check for Updates", "Sign In…", and "Quit" (no mount entries, no "Sign Out")

#### Scenario: Dynamic menu rebuild
- **WHEN** mount state changes (mount started, stopped, added, removed, toggled, or authentication state changes) or update state changes (update downloaded, update pending)
- **THEN** the system SHALL rebuild the tray context menu to reflect the current mount list, status, and update state; each mount entry displays the mount name and its current state (Mounted, Unmounted, or Error)

#### Scenario: Dynamic menu rebuild — mutex unavailable
- **WHEN** the tray menu rebuild is triggered but an internal mutex (effective config, active mounts, or update state) is unavailable (e.g., poisoned from a prior panic)
- **THEN** the system SHALL skip the rebuild for this invocation, log a warning, and return without panicking

#### Scenario: Open mount folder
- **WHEN** the user clicks on a mounted drive name in the tray menu
- **THEN** the system opens the mount point in the OS file manager (File Explorer, Finder, Nautilus/Dolphin)

#### Scenario: Mount/unmount toggle
- **WHEN** the user right-clicks a mount entry in the tray menu
- **THEN** the system shows "Unmount" for active mounts or "Mount" for inactive mounts, and executes the chosen action

#### Scenario: Check for updates action
- **WHEN** the user selects "Check for Updates" from the tray menu and no update is pending
- **THEN** the system checks for updates and notifies the user of the result

#### Scenario: Restart to update action
- **WHEN** the user selects "Restart to Update (v{version})" from the tray menu
- **THEN** the system performs graceful shutdown and installs the pending update

#### Scenario: Sign out action
- **WHEN** the user selects "Sign Out" from the tray menu and confirms the confirmation dialog
- **THEN** the system attempts to stop all active mounts, clear authentication tokens, remove account metadata, and save the config; regardless of whether any of these steps fail, the system SHALL set the authenticated state to false, transition the tray menu to the unauthenticated state (showing "Sign In…"), reload the settings window to clean DOM state, and show the sign-in wizard; if any cleanup step fails, the system SHALL emit a desktop notification describing the failure

#### Scenario: Sign in action
- **WHEN** the user selects "Sign In…" from the tray menu while not authenticated
- **THEN** the system opens the wizard window at step-welcome so the user can authenticate

#### Scenario: Quit action
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, stops the delta sync timer, and exits the process

### Requirement: First-run wizard
The system SHALL present a setup wizard on first launch (and whenever sign-in is required) to guide the user through account login and initial mount configuration. The wizard flow is the same for all accounts; there is no "branded" or "pre-configured" variant. During the sign-in flow, the wizard SHALL display the Microsoft auth URL with a copy button so the user can open it manually if the browser launch silently fails.

#### Scenario: First launch detected
- **WHEN** the application launches for the first time (no user configuration file exists) or no valid tokens are found on startup
- **THEN** the system opens a setup wizard window

#### Scenario: Wizard step sequence
- **WHEN** the wizard opens
- **THEN** it proceeds through these steps in order: (1) `step-welcome` — "Get started" landing with a "Sign in with Microsoft" button, (2) `step-sign-in` — PKCE browser flow; the auth URL is displayed with a copy button while the system awaits the OAuth callback, (3) `step-sources` — after successful sign-in, displays a unified source selection screen (see sharepoint-browser spec), (4) `step-success` — confirms mounts are activating

#### Scenario: Back navigation in wizard
- **WHEN** the user clicks "Back" on step-sources before finishing
- **THEN** the wizard does NOT return to step-sign-in (re-authentication is not triggered); back navigation from step-sources is not available; the user may cancel via the window close button which cancels any active sign-in flow

#### Scenario: Auth URL displayed during sign-in
- **WHEN** the wizard is on step-sign-in and the browser launch is initiated
- **THEN** the Microsoft auth URL is shown in the wizard with a copy button so the user can paste it manually if the browser does not open

#### Scenario: Sign-in cancel from wizard
- **WHEN** the user closes the wizard window while on step-sign-in
- **THEN** the system cancels the active PKCE flow and returns to the unauthenticated idle state

### Requirement: Branded UI elements
The system SHALL display the packaged branding throughout the UI when a custom app name is configured.

#### Scenario: Tray tooltip with custom name
- **WHEN** the packaged defaults define `app_name = "Contoso Drive"`
- **THEN** the system tray icon tooltip displays "Contoso Drive" instead of "CloudMount"

#### Scenario: Window titles with custom name
- **WHEN** the packaged defaults define a custom app name
- **THEN** the wizard window title, settings window title, and notification titles all use the custom name

#### Scenario: Default branding
- **WHEN** no custom app name is packaged
- **THEN** all UI elements display "CloudMount"

### Requirement: Notifications
The system SHALL display OS-native notifications for important events.

#### Scenario: Mount successful
- **WHEN** a drive is successfully mounted
- **THEN** the system displays a notification "{mountName} is now available at {path}"

#### Scenario: Sync conflict
- **WHEN** a file conflict is detected during sync
- **THEN** the system displays a notification "Conflict detected: {fileName}. A .conflict copy has been created."

#### Scenario: Authentication expired
- **WHEN** the authentication token cannot be refreshed
- **THEN** the system displays a notification "Sign-in expired. Click to re-authenticate." that opens the login flow when clicked

#### Scenario: Network error
- **WHEN** the network becomes unavailable for more than 30 seconds
- **THEN** the system displays a notification "Offline — cached files remain accessible. Changes will sync when connectivity returns."

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

### Requirement: Sign out action
The system SHALL stop all active mounts, clear authentication tokens, reload the wizard to its initial state, and reload the settings window to a clean state on sign-out. Sign-out from any entry point (tray menu or Account tab) MUST require explicit user confirmation before proceeding.

#### Scenario: Sign out action
- **WHEN** the user selects "Sign Out" from the tray menu
- **THEN** the system displays a native OS confirmation dialog ("Sign out? All mounts will stop."); if the user confirms, the system stops all active mounts (flushing pending writes), clears authentication tokens from the OS keyring and encrypted file fallback, removes account metadata from user config, saves the config, reloads the wizard window to its initial step-welcome state, reloads the settings window to a clean DOM state, and shows the sign-in wizard; if the user cancels, no action is taken

### Requirement: Minimize to tray
The system SHALL minimize to the system tray rather than closing when any window is closed, regardless of authentication state.

#### Scenario: Close window
- **WHEN** the user closes the settings window or wizard window (authenticated or not)
- **THEN** the window is hidden and the application continues running in the background as a tray icon; only "Quit" from the tray menu fully exits

#### Scenario: Quit application
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, and exits the process

### Requirement: Left-click tray icon routing
The system SHALL route the tray icon left-click to a destination that matches the current authentication state.

#### Scenario: Left-click when unauthenticated
- **WHEN** the user left-clicks the tray icon and `authenticated` is false
- **THEN** the system opens (or focuses) the wizard window at step-welcome, NOT the settings window

#### Scenario: Left-click when authenticated
- **WHEN** the user left-clicks the tray icon and `authenticated` is true
- **THEN** the system opens (or focuses) the settings window, preserving existing behavior

#### Scenario: Left-click when auth degraded
- **WHEN** the user left-clicks the tray icon and `auth_degraded` is true but `authenticated` is also true
- **THEN** the system opens (or focuses) the settings window (the tray menu "Re-authenticate…" item is the preferred recovery path)

### Requirement: Window minimum size
The system SHALL enforce a minimum inner window size of 640x480 pixels for all application windows (settings, wizard) created by `open_or_focus_window`.

#### Scenario: New window creation respects minimum size
- **WHEN** the system creates a new settings or wizard window
- **THEN** the window SHALL have a minimum inner size of 640x480 pixels and the user SHALL NOT be able to resize it smaller than this threshold

### Requirement: Auto-start failure notification
The system SHALL notify the user when auto-start registration or deregistration fails so they are aware the OS-level setting did not take effect.

#### Scenario: Notification content on failure
- **WHEN** the `autostart::set_enabled()` call returns an error
- **THEN** a desktop notification is displayed with a title of "Auto-start" and a body describing the failure (e.g., "Failed to register auto-start: systemctl not found"), using the same notification delivery mechanism as other application notifications
