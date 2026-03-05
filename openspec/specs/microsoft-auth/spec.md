### Requirement: OAuth2 PKCE authentication flow
The system SHALL authenticate users via OAuth2 Authorization Code Flow with PKCE using Microsoft Entra ID (Azure AD). Authentication MUST open the system default browser to Microsoft's login page and listen for the authorization code on a localhost redirect URI.

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

### Requirement: Silent token refresh
The system SHALL silently refresh expired access tokens using the stored refresh token without requiring user interaction.

#### Scenario: Access token near expiry
- **WHEN** an API call is about to be made and the access token expires within 5 minutes
- **THEN** the system proactively refreshes the token using the refresh token before making the API call

#### Scenario: Access token expired during request
- **WHEN** a Graph API call returns HTTP 401 Unauthorized
- **THEN** the system refreshes the access token and retries the failed request exactly once

#### Scenario: Refresh token expired or revoked
- **WHEN** a token refresh attempt fails with an invalid_grant error
- **THEN** the system switches all active mounts to read-only cached mode, displays a notification "Authentication expired — please sign in again", and presents the sign-in flow when the user clicks the notification

### Requirement: Secure token storage
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files.

#### Scenario: Token storage on Linux
- **WHEN** tokens are obtained after authentication on Linux
- **THEN** the system stores them via the Secret Service API (GNOME Keyring or KWallet) under the service name "filesync"

#### Scenario: Token storage on macOS
- **WHEN** tokens are obtained after authentication on macOS
- **THEN** the system stores them in the macOS Keychain under the service name "filesync"

#### Scenario: Token storage on Windows
- **WHEN** tokens are obtained after authentication on Windows
- **THEN** the system stores them in Windows Credential Manager under the target "filesync"

#### Scenario: Keychain unavailable fallback
- **WHEN** the OS credential store is unavailable (e.g., no keyring daemon on Linux)
- **THEN** the system stores tokens in an AES-256 encrypted file at the config directory, with the encryption key derived from a user-provided password, and warns the user that this is less secure

### Requirement: Sign out
The system SHALL allow the user to sign out, which revokes tokens and cleans up stored credentials.

#### Scenario: User signs out
- **WHEN** the user selects "Sign Out" from the tray menu or settings
- **THEN** the system unmounts all active mounts for that account, deletes the stored refresh token from the credential store, clears the in-memory access token, and returns to the unauthenticated state

### Requirement: Microsoft Graph permission scopes
The system SHALL request the minimum necessary permission scopes for its functionality.

#### Scenario: Requested scopes
- **WHEN** initiating the OAuth2 flow
- **THEN** the system requests exactly these scopes: `User.Read` (user profile), `Files.ReadWrite.All` (OneDrive file access), `Sites.Read.All` (SharePoint site discovery), `offline_access` (refresh token)

### Requirement: Client ID resolution
The system SHALL use the packaged client_id if available, falling back to a built-in default.

#### Scenario: Packaged client_id
- **WHEN** the packaged defaults contain a `[tenant]` section with `client_id`
- **THEN** the OAuth2 flow uses this client_id for all token requests

#### Scenario: No packaged client_id
- **WHEN** no client_id is configured in packaged defaults
- **THEN** the OAuth2 flow uses the built-in FileSync default app registration client_id
