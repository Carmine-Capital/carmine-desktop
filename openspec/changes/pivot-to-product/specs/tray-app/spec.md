## MODIFIED Requirements

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
