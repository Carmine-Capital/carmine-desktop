# ui-feedback Specification

## Purpose
TBD - created by archiving change fix-ui-feedback. Update Purpose after archive.
## Requirements
### Requirement: In-page toast notification system
The settings window SHALL display a status bar that shows transient in-page notifications for the outcome of every async operation. The notification system SHALL NOT depend on any external JavaScript library.

#### Scenario: Success toast shown after save
- **WHEN** `saveGeneral` or `saveAdvanced` completes successfully
- **THEN** a success toast ("Settings saved") is shown in the status bar and auto-hides after 3 seconds

#### Scenario: Error toast shown on save failure
- **WHEN** `saveGeneral` or `saveAdvanced` fails
- **THEN** an error toast displaying the error message is shown in the status bar and remains visible until the next user action

#### Scenario: Success toast shown after toggle mount
- **WHEN** `toggleMount` completes successfully
- **THEN** a success toast ("Mount updated") is shown in the status bar and auto-hides after 3 seconds

#### Scenario: Error toast shown on toggle mount failure
- **WHEN** `toggleMount` fails
- **THEN** an error toast displaying the error message is shown in the status bar and remains visible until the next user action

#### Scenario: Success toast shown after remove mount
- **WHEN** `removeMount` completes successfully
- **THEN** a success toast ("Mount removed") is shown in the status bar and auto-hides after 3 seconds

#### Scenario: Error toast shown on remove mount failure
- **WHEN** `removeMount` fails
- **THEN** an error toast displaying the error message is shown in the status bar and remains visible until the next user action

#### Scenario: Success toast shown after sign-out
- **WHEN** `signOut` in the settings Account tab completes successfully
- **THEN** a success toast ("Signed out") is shown briefly before the settings window is reloaded to a clean DOM state by the backend

#### Scenario: Error toast shown on sign-out failure
- **WHEN** `signOut` in the settings Account tab fails
- **THEN** an error toast displaying the error message is shown in the status bar and remains visible until the next user action

#### Scenario: clearCache uses toast instead of alert
- **WHEN** `clearCache` completes successfully
- **THEN** a success toast ("Cache cleared") is shown in the status bar (not via `alert()`)

#### Scenario: clearCache failure uses toast instead of alert
- **WHEN** `clearCache` fails
- **THEN** an error toast displaying the error message is shown in the status bar (not via `alert()`)

### Requirement: Button loading state during async operations
Every button that triggers an async backend operation in the settings window SHALL be disabled and display a loading label for the duration of the operation, preventing duplicate submissions.

#### Scenario: Save button shows loading state
- **WHEN** the user clicks "Save" in the General or Advanced tab
- **THEN** the button is immediately disabled and its label changes to "Saving…" until the operation resolves

#### Scenario: Remove button shows loading state
- **WHEN** the user clicks "Remove" for a mount
- **THEN** the button is immediately disabled and its label changes to "Removing…" until the operation resolves

#### Scenario: Toggle button shows loading state
- **WHEN** the user clicks "Enable" or "Disable" for a mount
- **THEN** the button is immediately disabled and its label changes to "Updating…" until the operation resolves

#### Scenario: Sign-out button shows loading state
- **WHEN** the user confirms sign-out in the Account tab
- **THEN** the "Sign Out" button is immediately disabled and its label changes to "Signing out…" until the operation resolves

#### Scenario: Button re-enabled after operation
- **WHEN** any async operation resolves (success or failure)
- **THEN** the button is re-enabled and its original label is restored

### Requirement: Confirmation dialog before destructive operations (settings)
The settings window SHALL require explicit user confirmation before executing any irreversible action. Confirmation in the settings window uses the browser-native `confirm()` dialog (available because settings runs inside a Tauri webview). Confirmation in the tray event handler — which has no webview context — uses `tauri-plugin-dialog` to show a native OS dialog instead; that requirement is specified in the `tray-app` delta spec for this change.

#### Scenario: Confirm before remove mount
- **WHEN** the user clicks "Remove" for a mount
- **THEN** a confirmation dialog ("Remove this mount? This cannot be undone.") is shown before invoking the backend; if the user cancels, no backend call is made

#### Scenario: Confirm before sign-out (settings)
- **WHEN** the user clicks "Sign Out" in the Account tab
- **THEN** a confirmation dialog ("Sign out? All mounts will stop.") is shown before invoking the backend; if the user cancels, no backend call is made

