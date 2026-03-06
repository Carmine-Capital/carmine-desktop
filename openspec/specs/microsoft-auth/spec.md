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
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files. The system SHALL verify that tokens were actually persisted after writing to the credential store. The storage key used for storing, loading, refreshing, and deleting tokens SHALL be the application client_id — consistent across all token lifecycle operations.

#### Scenario: Token storage on Linux
- **WHEN** tokens are obtained after authentication on Linux
- **THEN** the system stores them via the Secret Service API (GNOME Keyring or KWallet) under the service name "cloudmount"

#### Scenario: Token storage on macOS
- **WHEN** tokens are obtained after authentication on macOS
- **THEN** the system stores them in the macOS Keychain under the service name "cloudmount"

#### Scenario: Token storage on Windows
- **WHEN** tokens are obtained after authentication on Windows
- **THEN** the system stores them in Windows Credential Manager under the target "cloudmount"

#### Scenario: Verify-after-write for credential store
- **WHEN** the system writes tokens to the OS credential store and the write call returns success
- **THEN** the system SHALL immediately read back the stored value from the credential store and compare it to the original serialized data; if the read-back fails or returns different data, the system treats the credential store as unreliable and falls through to the encrypted file fallback

#### Scenario: Keychain unavailable fallback
- **WHEN** the OS credential store is unavailable (e.g., no keyring daemon on Linux), or the credential store accepts a write but fails the verify-after-write check (e.g., session-scoped storage, locked collection, null backend)
- **THEN** the system stores tokens in an AES-256 encrypted file at the config directory, with the encryption key derived from a machine-specific identifier, and warns the user that this is less secure

#### Scenario: Consistent storage key across token operations
- **WHEN** the system stores, loads, refreshes, or deletes tokens in the credential store or encrypted file fallback
- **THEN** all operations SHALL use the application client_id as the credential key; the token restoration method (`try_restore`) SHALL NOT use a caller-provided account identifier as the storage lookup key, as this would cause a key mismatch with the store operation which uses the client_id

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
