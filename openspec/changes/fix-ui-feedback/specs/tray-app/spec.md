## MODIFIED Requirements

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
- **THEN** the system displays a native OS confirmation dialog ("Sign out? All mounts will stop."); if the user confirms, the system stops all active mounts (flushing pending writes), clears authentication tokens from the OS keyring and encrypted file fallback, removes account metadata from user config, saves the config, reloads the wizard window to its initial step-welcome state, reloads the settings window to a clean DOM state, and shows the sign-in wizard; if the user cancels, no action is taken

#### Scenario: Sign in action
- **WHEN** the user selects "Sign In…" from the tray menu while not authenticated
- **THEN** the system opens the wizard window at step-welcome so the user can authenticate

#### Scenario: Quit action
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, stops the delta sync timer, and exits the process

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

#### Scenario: Settings operation feedback — success
- **WHEN** the user completes any settings operation (save general, save advanced, toggle mount, remove mount, clear cache, sign out) and the operation succeeds
- **THEN** the settings window displays an in-page success notification confirming the outcome

#### Scenario: Settings operation feedback — failure
- **WHEN** any settings operation fails
- **THEN** the settings window displays an in-page error notification describing the failure; the error remains visible until the user takes the next action
