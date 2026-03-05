## MODIFIED Requirements

### Requirement: First-run wizard
The system SHALL present a setup wizard on first launch to guide the user through account login and initial mount configuration. The wizard adapts its flow based on whether packaged defaults are present.

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
- **WHEN** the user cancels the wizard at any step
- **THEN** the system exits cleanly without creating any configuration

#### Scenario: Root directory conflict during wizard
- **WHEN** the suggested root directory path already exists on the filesystem
- **THEN** the system displays a warning "~/Cloud already exists — files inside won't be affected" and allows the user to choose a different name or proceed with the existing directory

### Requirement: Tray context menu
The system SHALL provide a context menu when the user right-clicks (or clicks on macOS) the tray icon.

#### Scenario: Context menu contents
- **WHEN** the user activates the tray icon context menu
- **THEN** the menu displays: the list of configured mounts with their status (mounted/unmounted/error), a separator, "Add Mount...", "Settings...", "Sign Out", and "Quit"

#### Scenario: Open mount folder
- **WHEN** the user clicks on a mounted drive name in the tray menu
- **THEN** the system opens the mount point in the OS file manager (File Explorer, Finder, Nautilus/Dolphin)

#### Scenario: Mount/unmount toggle
- **WHEN** the user right-clicks a mount entry in the tray menu
- **THEN** the system shows "Unmount" for active mounts or "Mount" for inactive mounts, and executes the chosen action

#### Scenario: Sign out action
- **WHEN** the user selects "Sign Out" from the tray menu
- **THEN** the system stops all active mounts (flushing pending writes), clears authentication tokens from the OS keyring and encrypted file fallback, removes account metadata from user config, saves the config, and shows the sign-in wizard

#### Scenario: Quit action
- **WHEN** the user selects "Quit" from the tray menu
- **THEN** the system flushes all pending writes, unmounts all drives, stops the delta sync timer, and exits the process
