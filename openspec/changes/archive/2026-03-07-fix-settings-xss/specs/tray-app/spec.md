## MODIFIED Requirements

### Requirement: Settings window
The system SHALL provide a settings window accessible from the tray menu. All dynamic content displayed in the settings window that originates from user configuration or Tauri IPC responses MUST be rendered using safe DOM APIs (`textContent`, `createElement`, `addEventListener`/closure-bound handlers). No user-supplied string SHALL be interpolated into HTML markup strings or inline event handler attributes.

#### Scenario: Open settings
- **WHEN** the user selects "Settings..." from the tray menu
- **THEN** a settings window opens with tabs: General, Mounts, Account, Advanced

#### Scenario: General settings
- **WHEN** the user views the General tab
- **THEN** they can configure: auto-start on login (toggle), notification preferences (toggle), and global sync interval (dropdown: 30s, 1m, 5m, 15m)

#### Scenario: Mount settings
- **WHEN** the user views the Mounts tab
- **THEN** they see a list of all configured mounts with controls to enable/disable, change mount point, remove, and add new mounts; mount names, paths, and IDs are displayed as plain text without HTML interpretation

#### Scenario: Mount list renders safely with adversarial config values
- **WHEN** a mount entry in the user config contains HTML markup or JavaScript in its name, path, or ID fields
- **THEN** the settings window renders those values as literal text without executing any script or creating unintended DOM elements; button actions continue to function correctly using the actual field values

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
