## MODIFIED Requirements

### Requirement: Settings window
The system SHALL provide a settings window accessible from the tray menu. The settings window SHALL always display current persisted state when opened or re-shown — it SHALL NOT display unsaved form values from a previous session or account information from before a sign-out.

#### Scenario: Open settings
- **WHEN** the user selects "Settings..." from the tray menu
- **THEN** a settings window opens with tabs: General, Mounts, Account, Advanced

#### Scenario: General settings
- **WHEN** the user views the General tab
- **THEN** they can configure: auto-start on login (toggle), notification preferences (toggle), and global sync interval (dropdown: 30s, 1m, 5m, 15m)

#### Scenario: Mount settings
- **WHEN** the user views the Mounts tab
- **THEN** they see a list of all configured mounts with controls to enable/disable, change mount point, remove, and add new mounts

#### Scenario: Account tab — signed in
- **WHEN** the user views the Account tab and is authenticated
- **THEN** they see the account display name (or email if available) and a "Sign Out" button

#### Scenario: Account tab — not signed in
- **WHEN** the user views the Account tab and is NOT authenticated
- **THEN** they see "Not signed in" text and no "Sign Out" button (or a disabled one)

#### Scenario: Sign out from Account tab
- **WHEN** the user clicks "Sign Out" in the Account tab
- **THEN** the system displays a confirmation dialog ("Sign out? All mounts will stop."); if the user confirms, the system performs sign-out (stops mounts, clears tokens, updates config), reloads the settings window to a clean DOM state, and opens the wizard at step-welcome; if the user cancels, no action is taken

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
- **WHEN** the user closes the wizard window before completing authentication
- **THEN** the wizard window is hidden and the application continues running as a tray-only process; the user can reopen the wizard at any time via "Sign In…" from the tray menu; the process SHALL NOT exit

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
- **THEN** the wizard returns to the initial sign-in screen, the PKCE listener is abandoned, the auth URL input is cleared, and any error message from a prior attempt is hidden so the welcome step is presented in a clean state

### Requirement: Window minimum size
The system SHALL enforce a minimum inner window size of 640x480 pixels for all application windows (settings, wizard) created by `open_or_focus_window`.

#### Scenario: New window creation respects minimum size
- **WHEN** the system creates a new settings or wizard window
- **THEN** the window SHALL have a minimum inner size of 640x480 pixels and the user SHALL NOT be able to resize it smaller than this threshold
