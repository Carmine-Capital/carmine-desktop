## MODIFIED Requirements

### Requirement: Tray context menu
The system SHALL provide a context menu when the user right-clicks (or clicks on macOS) the tray icon.

#### Scenario: Context menu contents
- **WHEN** the user activates the tray icon context menu and is authenticated
- **THEN** the menu displays: the list of configured mounts with their status (mounted/unmounted/error), a separator, "Add Mount...", "Settings...", "Check for Updates", "Sign Out", and "Quit"

#### Scenario: Context menu contents — unauthenticated
- **WHEN** the user activates the tray icon context menu and is NOT authenticated
- **THEN** the menu displays: "Settings...", "Check for Updates", "Sign In…", and "Quit" (no mount entries, no "Sign Out")

#### Scenario: Context menu contents — auth degraded
- **WHEN** the user activates the tray icon context menu and `auth_degraded` is true (authenticated but token refresh has failed)
- **THEN** the menu displays all standard authenticated items AND additionally a "Re-authenticate…" item placed immediately before "Sign Out"; the "Sign Out" item remains present

#### Scenario: Dynamic menu rebuild
- **WHEN** mount state changes (mount started, stopped, added, removed, toggled, or authentication state changes) or update state changes (update downloaded, update pending)
- **THEN** the system SHALL rebuild the tray context menu to reflect the current mount list, status, and update state; each mount entry displays the mount name and its current state (Mounted, Unmounted, or Error); mount count shown in the tooltip SHALL be derived from an explicit boolean `is_mounted` field, not from substring matching of the display label

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

#### Scenario: Re-authenticate action
- **WHEN** the user selects "Re-authenticate…" from the tray menu while `auth_degraded` is true
- **THEN** the system opens the wizard window at step-welcome so the user can sign in again without first signing out

#### Scenario: Sign in action
- **WHEN** the user selects "Sign In…" from the tray menu while not authenticated
- **THEN** the system opens the wizard window at step-welcome so the user can authenticate

#### Scenario: Quit action
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, stops the delta sync timer, and exits the process

## ADDED Requirements

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
