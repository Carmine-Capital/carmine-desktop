## MODIFIED Requirements

### Requirement: OAuth2 PKCE authentication flow
The system SHALL authenticate users via OAuth2 Authorization Code Flow with PKCE using Microsoft Entra ID (Azure AD). Authentication MUST open the user's default browser to Microsoft's login page using a caller-provided URL opener mechanism, and listen for the authorization code on a localhost redirect URI. The URL opener SHALL be injected into the auth module at construction time, allowing the caller to provide a platform-appropriate implementation. On Linux in desktop mode, the opener SHALL spawn `xdg-open` directly via `std::process::Command` with `LD_LIBRARY_PATH` and `LD_PRELOAD` removed from the child process environment, and wait for the exit code. On macOS and Windows in desktop mode, `tauri-plugin-opener` SHALL be used. In headless mode, `open::that()` SHALL be used. Before invoking the opener, the system SHALL communicate the auth URL to any registered listener (e.g., the wizard UI) so it can be displayed as a fallback.

#### Scenario: First-time login (generic build)
- **WHEN** the user clicks "Sign In", has no existing tokens, and no packaged tenant is configured
- **THEN** the system invokes the injected URL opener with the Microsoft OAuth2 authorize endpoint (`login.microsoftonline.com/common/oauth2/v2.0/authorize`) with PKCE challenge, client_id, redirect_uri (http://localhost:{dynamic_port}/callback), and scopes (User.Read, Files.ReadWrite.All, Sites.Read.All, offline_access)

#### Scenario: First-time login (pre-configured tenant)
- **WHEN** the user clicks "Sign In", has no existing tokens, and packaged defaults define a tenant_id
- **THEN** the system invokes the injected URL opener with the tenant-specific authorize endpoint (`login.microsoftonline.com/{tenant_id}/oauth2/v2.0/authorize`) with `domain_hint={tenant_id}` and `login_hint` parameters, so the Microsoft login page skips org selection and goes directly to the correct tenant

#### Scenario: Successful authentication callback
- **WHEN** the user completes login in the browser and is redirected to the localhost callback with an authorization code
- **THEN** the system exchanges the code for an access token and refresh token via the token endpoint, stores both tokens securely, and signals authentication success

#### Scenario: Authentication failure
- **WHEN** the browser callback returns an error (user denied consent, admin policy blocked, network error)
- **THEN** the system displays a clear error message describing the failure reason and allows the user to retry

#### Scenario: User cancels login
- **WHEN** the user closes the browser window without completing login, and the localhost listener times out after 120 seconds
- **THEN** the system cancels the authentication attempt and returns to the unauthenticated state

#### Scenario: Desktop mode browser opening on Linux
- **WHEN** the OAuth flow is initiated in desktop mode (Tauri) on Linux
- **THEN** the system spawns `xdg-open {auth_url}` via `std::process::Command` with `LD_LIBRARY_PATH` and `LD_PRELOAD` removed from the child environment, and waits for the process to exit; if the exit code is non-zero, the opener returns an error

#### Scenario: Desktop mode browser opening on macOS or Windows
- **WHEN** the OAuth flow is initiated in desktop mode (Tauri) on macOS or Windows
- **THEN** the system uses `tauri-plugin-opener` to open the auth URL

#### Scenario: Headless mode browser opening
- **WHEN** the OAuth flow is initiated in headless mode and a display server is detected (Linux: `$DISPLAY` or `$WAYLAND_DISPLAY` is set)
- **THEN** the system uses `open::that()` to open the auth URL in the default browser

#### Scenario: No display server available (headless)
- **WHEN** the OAuth flow is initiated in headless mode and no display server is detected, or the opener fails
- **THEN** the system prints to stderr: "Open this URL in your browser to sign in:\n\n  {auth_url}\n\nWaiting for authentication..." and continues listening on the localhost callback for the redirect

#### Scenario: URL opener failure in desktop mode
- **WHEN** the injected URL opener returns an error in desktop mode
- **THEN** the system SHALL log the error with warn level; the wizard UI SHALL already have the auth URL displayed (per the auth URL forwarding requirement) so the user can copy-paste it; the system continues waiting for the localhost callback

#### Scenario: Display detection on non-Linux platforms
- **WHEN** the OAuth flow is initiated on macOS or Windows
- **THEN** the system always attempts to open the browser via the injected opener; if it fails unexpectedly, it falls back to printing the URL to stderr

## ADDED Requirements

### Requirement: Auth URL forwarding to UI during PKCE flow
Before blocking on the localhost callback, the system SHALL forward the constructed auth URL to a registered channel so that callers (e.g., the wizard UI) can display it. This allows the user to copy-paste the URL if the browser launch silently fails or if they prefer to open it manually.

#### Scenario: Auth URL sent before browser is opened
- **WHEN** `run_pkce_flow` constructs the auth URL and before calling the opener
- **THEN** if a URL channel (`oneshot::Sender<String>`) was provided, the system SHALL send the auth URL on that channel; the send MUST happen before `wait_for_callback` is called so the caller receives the URL while the flow is still active

#### Scenario: No channel registered
- **WHEN** no URL channel is provided to the PKCE flow
- **THEN** the system proceeds normally without sending a URL; behaviour is identical to current

#### Scenario: Auth URL channel send failure
- **WHEN** the URL channel's receiver has already been dropped when the sender fires
- **THEN** the system logs a debug message and continues the PKCE flow normally; a dropped receiver is not an error
