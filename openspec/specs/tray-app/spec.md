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
- **WHEN** the user activates the tray icon context menu
- **THEN** the menu displays: the list of configured mounts with their status (mounted/unmounted/error), a separator, "Add Mount...", "Settings...", "Check for Updates", "Sign Out", and "Quit"

#### Scenario: Dynamic menu rebuild
- **WHEN** mount state changes (mount started, stopped, added, removed, toggled, or authentication state changes) or update state changes (update downloaded, update pending)
- **THEN** the system SHALL rebuild the tray context menu to reflect the current mount list, status, and update state; each mount entry displays the mount name and its current state (Mounted, Unmounted, or Error)

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
- **WHEN** the user selects "Sign Out" from the tray menu
- **THEN** the system stops all active mounts (flushing pending writes), clears authentication tokens from the OS keyring and encrypted file fallback, removes account metadata from user config, saves the config, and shows the sign-in wizard

#### Scenario: Quit action
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, stops the delta sync timer, and exits the process

### Requirement: First-run wizard
The system SHALL present a setup wizard on first launch to guide the user through account login and initial mount configuration. The wizard adapts its flow based on whether packaged defaults are present. During the sign-in flow, the wizard SHALL display the Microsoft auth URL with a copy button so the user can open it manually if the browser launch silently fails.

#### Scenario: First launch detected
- **WHEN** the application launches for the first time (no user configuration file exists)
- **THEN** the system opens a setup wizard window instead of going directly to background mode

#### Scenario: Full wizard flow (no packaged defaults)
- **WHEN** the wizard is displayed and the binary has no packaged tenant or mount configuration
- **THEN** it guides the user through: (1) "Sign in with Microsoft" button as the sole UI element, (2) after successful sign-in, the system auto-discovers the user's OneDrive and prompts for a root directory name (default "Cloud", with a warning if `~/Cloud` already exists), (3) the system automatically creates and mounts OneDrive at `~/{root_dir}/OneDrive/`, (4) a success screen shows "Your OneDrive is ready" with a note "Add SharePoint libraries anytime from Settings"

#### Scenario: Pre-configured wizard flow (packaged defaults present)
- **WHEN** the wizard is displayed and the binary has packaged defaults with a tenant and mounts
- **THEN** the wizard shows a simplified flow: (1) Welcome screen showing the branded app name and the list of pre-configured drives, (2) "Sign in with Microsoft" button (tenant pre-locked via domain_hint), (3) After sign-in: all packaged mounts are automatically activated and mounted, (4) Success screen showing mounted drives with a note "You can add more drives in Settings anytime"

#### Scenario: Pre-configured wizard completion
- **WHEN** the user completes sign-in in the pre-configured wizard flow
- **THEN** the system auto-mounts all enabled packaged mounts, minimizes to the system tray, and shows a notification listing the mounted drives

#### Scenario: Wizard cancellation
- **WHEN** the user closes the wizard window before completing authentication (during first-run)
- **THEN** the system SHALL exit the process cleanly without creating any configuration; specifically, if the user has not yet authenticated when the wizard window close is requested, the application exits with code 0 instead of hiding the window

#### Scenario: Root directory conflict during wizard
- **WHEN** the suggested root directory path already exists on the filesystem
- **THEN** the system displays a warning "~/Cloud already exists — files inside won't be affected" and allows the user to choose a different name or proceed with the existing directory

#### Scenario: Auth URL displayed during sign-in
- **WHEN** the user clicks "Sign In" in the wizard and the PKCE flow begins
- **THEN** the wizard SHALL transition to a waiting state that displays the auth URL and a "Copy URL" button; the user can click "Copy URL" to copy the auth URL to the clipboard and open it manually in any browser; the wizard continues waiting for the localhost callback

#### Scenario: Auth URL copy action
- **WHEN** the user clicks "Copy URL" in the wizard sign-in waiting state
- **THEN** the auth URL is copied to the system clipboard and a brief confirmation ("Copied!") is shown

#### Scenario: Sign-in waiting state cancelled
- **WHEN** the user clicks "Cancel" in the sign-in waiting state (before the 120s timeout)
- **THEN** the wizard returns to the initial sign-in screen and the PKCE listener is abandoned

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
The system SHALL provide a settings window accessible from the tray menu.

#### Scenario: Open settings
- **WHEN** the user selects "Settings..." from the tray menu
- **THEN** a settings window opens with tabs: General, Mounts, Account, Advanced

#### Scenario: General settings
- **WHEN** the user views the General tab
- **THEN** they can configure: auto-start on login (toggle), notification preferences (toggle), and global sync interval (dropdown: 30s, 1m, 5m, 15m)

#### Scenario: Mount settings
- **WHEN** the user views the Mounts tab
- **THEN** they see a list of all configured mounts with controls to enable/disable, change mount point, remove, and add new mounts

#### Scenario: Advanced settings
- **WHEN** the user views the Advanced tab
- **THEN** they can configure: cache directory path, maximum cache size, metadata TTL, debug logging toggle, and a "Clear Cache" button

### Requirement: Minimize to tray
The system SHALL minimize to the system tray rather than closing when the main window is closed.

#### Scenario: Close window
- **WHEN** the user closes the settings window or wizard window
- **THEN** the application continues running in the background as a tray icon; only "Quit" from the tray menu fully exits

#### Scenario: Quit application
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, and exits the process
