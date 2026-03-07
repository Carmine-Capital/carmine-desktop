## MODIFIED Requirements

### Requirement: OAuth2 PKCE authentication flow
The system SHALL authenticate users via OAuth2 Authorization Code Flow with PKCE using Microsoft Entra ID (Azure AD). Authentication MUST open the user's default browser to Microsoft's login page using a caller-provided URL opener mechanism, and listen for the authorization code on a localhost redirect URI. The URL opener SHALL be injected into the auth module at construction time, allowing the caller to provide a platform-appropriate implementation (e.g., Tauri opener plugin for desktop mode, `open::that()` for headless mode).

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

#### Scenario: Desktop mode browser opening
- **WHEN** the OAuth flow is initiated in desktop mode (Tauri)
- **THEN** the system uses the Tauri opener plugin (`tauri-plugin-opener`) to open the auth URL, which goes through `xdg-desktop-portal` on Wayland and works correctly in AppImage and sandboxed environments

#### Scenario: Headless mode browser opening
- **WHEN** the OAuth flow is initiated in headless mode and a display server is detected (Linux: `$DISPLAY` or `$WAYLAND_DISPLAY` is set)
- **THEN** the system uses `open::that()` to open the auth URL in the default browser

#### Scenario: No display server available (headless)
- **WHEN** the OAuth flow is initiated in headless mode and no display server is detected, or the opener fails
- **THEN** the system prints to stderr: "Open this URL in your browser to sign in:\n\n  {auth_url}\n\nWaiting for authentication..." and continues listening on the localhost callback for the redirect

#### Scenario: URL opener failure in desktop mode
- **WHEN** the injected URL opener returns an error in desktop mode
- **THEN** the system SHALL log the error and fall back to printing the auth URL to stderr

#### Scenario: Display detection on non-Linux platforms
- **WHEN** the OAuth flow is initiated on macOS or Windows
- **THEN** the system always attempts to open the browser via the injected opener; if it fails unexpectedly, it falls back to printing the URL to stderr
