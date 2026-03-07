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
