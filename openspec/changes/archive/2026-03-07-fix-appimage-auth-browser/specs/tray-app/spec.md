## MODIFIED Requirements

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
