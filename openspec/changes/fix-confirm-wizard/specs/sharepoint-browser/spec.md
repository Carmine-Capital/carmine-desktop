## ADDED Requirements

### Requirement: Wizard auth-aware routing
The wizard window SHALL detect the current authentication state on load and route the user to the appropriate starting step. If the user is already authenticated when the wizard opens, the wizard SHALL bypass the sign-in step and navigate directly to `step-sources`.

#### Scenario: Wizard opened when already authenticated
- **WHEN** the wizard window is created or navigated while `is_authenticated` returns true
- **THEN** the wizard SHALL skip `step-welcome` and transition to `step-sources`, calling `loadSources()` to populate OneDrive and SharePoint options

#### Scenario: Wizard opened when not authenticated
- **WHEN** the wizard window is created and `is_authenticated` returns false
- **THEN** the wizard SHALL display `step-welcome` with the "Sign in with Microsoft" button, following the normal sign-in flow

#### Scenario: Re-focused existing wizard navigated to step-sources for add-mount
- **WHEN** the wizard window already exists and "Add Mount" is triggered (from the settings Mounts tab or the tray menu)
- **THEN** the system SHALL call `goToAddMount()` on the existing wizard window via `win.eval()`, navigating it to `step-sources` regardless of which step it was previously displaying
