## MODIFIED Requirements

### Requirement: OAuth2 PKCE authentication flow
The system SHALL authenticate users via OAuth2 Authorization Code Flow with PKCE using Microsoft Entra ID (Azure AD). Authentication MUST open the system default browser to Microsoft's login page and listen for the authorization code on a localhost redirect URI. When the browser cannot be opened (no display server, headless environment), the system SHALL print the authorization URL to stdout for manual copy-paste.

#### Scenario: First-time login (generic build)
- **WHEN** the user clicks "Sign In", has no existing tokens, and no packaged tenant is configured
- **THEN** the system opens the default browser to the Microsoft OAuth2 authorize endpoint (`login.microsoftonline.com/common/oauth2/v2.0/authorize`) with PKCE challenge, client_id, redirect_uri (http://localhost:{dynamic_port}/callback), and scopes (User.Read, Files.ReadWrite.All, Sites.Read.All, offline_access)

#### Scenario: First-time login (pre-configured tenant)
- **WHEN** the user clicks "Sign In", has no existing tokens, and packaged defaults define a tenant_id
- **THEN** the system opens the default browser to the tenant-specific authorize endpoint (`login.microsoftonline.com/{tenant_id}/oauth2/v2.0/authorize`) with `domain_hint={tenant_id}` and `login_hint` parameters, so the Microsoft login page skips org selection and goes directly to the correct tenant

#### Scenario: Successful authentication callback
- **WHEN** the user completes login in the browser and is redirected to the localhost callback with an authorization code
- **THEN** the system exchanges the code for an access token and refresh token via the token endpoint, stores both tokens securely, and signals authentication success

#### Scenario: Authentication failure
- **WHEN** the browser callback returns an error (user denied consent, admin policy blocked, network error)
- **THEN** the system displays a clear error message describing the failure reason and allows the user to retry

#### Scenario: User cancels login
- **WHEN** the user closes the browser window without completing login, and the localhost listener times out after 120 seconds
- **THEN** the system cancels the authentication attempt and returns to the unauthenticated state

#### Scenario: No display server available
- **WHEN** the OAuth flow is initiated and no display server is detected (Linux: neither `$DISPLAY` nor `$WAYLAND_DISPLAY` is set) or `open::that()` fails to open the browser
- **THEN** the system prints to stdout: "Open this URL in your browser to sign in:\n\n  {auth_url}\n\nWaiting for authentication..." and continues listening on the localhost callback for the redirect

#### Scenario: Display detection on non-Linux platforms
- **WHEN** the OAuth flow is initiated on macOS or Windows
- **THEN** the system always attempts `open::that()` first (both platforms always have a display server); if it fails unexpectedly, it falls back to printing the URL to stdout

### Requirement: Client ID resolution
The system SHALL resolve the client_id using a four-layer precedence chain: CLI argument, environment variable, packaged defaults, built-in default.

#### Scenario: Client ID from CLI argument
- **WHEN** the `--client-id` CLI argument is provided
- **THEN** the OAuth2 flow uses this client_id, overriding all other sources

#### Scenario: Client ID from environment variable
- **WHEN** `CLOUDMOUNT_CLIENT_ID` is set and no `--client-id` CLI argument is provided
- **THEN** the OAuth2 flow uses the environment variable value

#### Scenario: Packaged client_id
- **WHEN** the packaged defaults contain a `[tenant]` section with `client_id` and no CLI or env override is provided
- **THEN** the OAuth2 flow uses this client_id for all token requests

#### Scenario: No client_id configured
- **WHEN** no client_id is configured via CLI, environment, or packaged defaults
- **THEN** the system falls back to the built-in default client_id
