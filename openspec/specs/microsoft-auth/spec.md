## Purpose
Defines OAuth2 PKCE authentication, token refresh, secure storage, and sign-out for Microsoft 365 organizational accounts.
## Requirements
### Requirement: OAuth2 PKCE authentication flow
The system SHALL authenticate users via OAuth2 Authorization Code Flow with PKCE using Microsoft Entra ID (Azure AD). Authentication MUST open the user's default browser to Microsoft's login page using a caller-provided URL opener mechanism, and listen for the authorization code on a localhost redirect URI. The URL opener SHALL be injected into the auth module at construction time, allowing the caller to provide a platform-appropriate implementation. On Linux in desktop mode, the opener SHALL spawn `xdg-open` directly via `std::process::Command` with `LD_LIBRARY_PATH` and `LD_PRELOAD` removed from the child process environment, and wait for the exit code. On macOS and Windows in desktop mode, `tauri-plugin-opener` SHALL be used. In headless mode, `open::that()` SHALL be used. Before invoking the opener, the system SHALL communicate the auth URL to any registered listener (e.g., the wizard UI) so it can be displayed as a fallback. The system SHALL enforce that at most one PKCE flow runs at a time; if a flow is already active when `sign_in()` is called, the prior flow MUST be cancelled before the new one begins.

#### Scenario: First-time login
- **WHEN** the user clicks "Sign In" and has no existing tokens
- **THEN** the system invokes the injected URL opener with the Microsoft OAuth2 authorize endpoint (`login.microsoftonline.com/common/oauth2/v2.0/authorize`) with PKCE challenge, the official carminedesktop client_id, redirect_uri (http://localhost:{dynamic_port}/callback), and scopes (User.Read, Files.ReadWrite.All, Sites.Read.All, offline_access); the `common` endpoint supports both M365 organizational accounts and personal Microsoft accounts (MSA)

#### Scenario: Successful authentication callback
- **WHEN** the user completes login in the browser and is redirected to the localhost callback with an authorization code
- **THEN** the system exchanges the code for an access token and refresh token via the token endpoint, stores both tokens securely, and signals authentication success

#### Scenario: Authentication failure
- **WHEN** the browser callback returns an error (user denied consent, admin policy blocked, network error)
- **THEN** the system displays a clear error message describing the failure reason and allows the user to retry

#### Scenario: User cancels login
- **WHEN** the user closes the browser window without completing login, and the localhost listener times out after 120 seconds
- **THEN** the system cancels the authentication attempt and returns to the unauthenticated state

#### Scenario: User cancels login via UI
- **WHEN** the user clicks the Cancel button in the sign-in wizard before the OAuth callback is received
- **THEN** the system SHALL immediately terminate the active PKCE flow by firing its cancellation token, stop waiting for the localhost callback, and return to the unauthenticated state without waiting for the 120-second timeout

#### Scenario: Concurrent sign-in attempt — cancel and retry
- **WHEN** `sign_in()` is called while a previous `sign_in()` call is still waiting for the OAuth callback
- **THEN** the system SHALL cancel the prior flow (fire its cancellation token), then begin a new PKCE flow; only one flow SHALL be active at any given time

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
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files. The system SHALL verify that tokens were actually persisted after writing to the credential store. Tokens SHALL be stored and loaded using the user's account ID (drive ID) as the storage key. Immediately after token exchange (before the account ID is known), tokens are transiently stored under the OAuth client ID key; `AuthManager::finalize_sign_in` MUST be called once the account ID is known to migrate tokens to the permanent account ID key.

#### Scenario: Token storage on Linux
- **WHEN** tokens are obtained after authentication on Linux
- **THEN** the system stores them via the Secret Service API (GNOME Keyring or KWallet) under the service name "carminedesktop"

#### Scenario: Token storage on macOS
- **WHEN** tokens are obtained after authentication on macOS
- **THEN** the system stores them in the macOS Keychain under the service name "carminedesktop"

#### Scenario: Token storage on Windows
- **WHEN** tokens are obtained after authentication on Windows
- **THEN** the system stores them in Windows Credential Manager under the target "carminedesktop"

#### Scenario: Verify-after-write for credential store
- **WHEN** the system writes tokens to the OS credential store and the write call returns success
- **THEN** the system SHALL immediately read back the stored value from the credential store and compare it to the original serialized data; if the read-back fails or returns different data, the system treats the credential store as unreliable and falls through to the encrypted file fallback

#### Scenario: Keychain unavailable fallback
- **WHEN** the OS credential store is unavailable (e.g., no keyring daemon on Linux), or the credential store accepts a write but fails the verify-after-write check (e.g., session-scoped storage, locked collection, null backend)
- **THEN** the system stores tokens in an AES-256 encrypted file at the config directory, with the encryption key derived from a machine-specific identifier, and warns the user that this is less secure

#### Scenario: Permanent storage key is account ID after finalization
- **WHEN** `finalize_sign_in(account_id)` has been called after a successful sign-in
- **THEN** all subsequent token operations (load, refresh, delete) SHALL use the account ID as the storage key; the OAuth client ID SHALL NOT be used as a storage key after finalization

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
The system SHALL use the official carminedesktop client ID (`8ebe3ef7-f509-4146-8fef-c9b5d7c22252`) as a hardcoded constant. A `--client-id` CLI argument MAY override this value for development and testing purposes. No build-time env vars or packaged defaults participate in client ID resolution.

#### Scenario: Client ID from CLI argument (development override)
- **WHEN** the `--client-id` CLI argument is provided
- **THEN** the OAuth2 flow uses this client_id, overriding the hardcoded constant

#### Scenario: No CLI override — official client ID used
- **WHEN** no `--client-id` CLI argument is provided
- **THEN** the OAuth2 flow uses the hardcoded official carminedesktop client ID constant

### Requirement: Sign-in cancellation API
The system SHALL expose a `cancel()` method on `AuthManager` that, when called, immediately terminates any active PKCE flow. This method SHALL be callable from any context (including from a Tauri command handler) and SHALL be a no-op if no flow is currently active.

#### Scenario: Cancel with active flow
- **WHEN** `AuthManager::cancel()` is called while a PKCE flow is waiting for the OAuth callback
- **THEN** the flow SHALL stop waiting, the `sign_in()` future SHALL return an `Err` with a cancellation message, and the internal cancellation token SHALL be cleared so the next `sign_in()` call starts fresh

#### Scenario: Cancel with no active flow
- **WHEN** `AuthManager::cancel()` is called when no PKCE flow is in progress
- **THEN** the call SHALL return immediately without error or side effects

### Requirement: Wizard cancel_sign_in command
The system SHALL provide a `cancel_sign_in` Tauri command that, when invoked, cancels the active backend sign-in flow and aborts the associated async spawn. The wizard's Cancel button SHALL invoke this command before performing any frontend cleanup.

#### Scenario: Cancel button pressed during sign-in
- **WHEN** the user clicks Cancel in the wizard while a sign-in flow is in progress
- **THEN** the frontend SHALL invoke `cancel_sign_in`, which calls `AuthManager::cancel()` and aborts the backend spawn; the frontend then resets its state and returns to the welcome step

#### Scenario: cancel_sign_in with no active spawn
- **WHEN** `cancel_sign_in` is invoked but no sign-in spawn is tracked in `AppState`
- **THEN** the command SHALL return `Ok(())` without error

### Requirement: Exclusive sign-in spawn tracking
The system SHALL track the `JoinHandle` of the most recent `start_sign_in` spawn in `AppState`. Before starting a new sign-in spawn, any previously tracked handle SHALL be aborted. This provides a belt-and-suspenders guarantee alongside the `CancellationToken` in `AuthManager`.

#### Scenario: start_sign_in called with a prior spawn still running
- **WHEN** `start_sign_in` is invoked while a previous spawn handle is tracked in `AppState`
- **THEN** the prior handle SHALL be aborted before the new spawn is created; only the new spawn's handle SHALL be tracked going forward

#### Scenario: start_sign_in called with no prior spawn
- **WHEN** `start_sign_in` is invoked and no prior spawn handle is tracked
- **THEN** the new spawn proceeds normally and its handle is stored in `AppState`

