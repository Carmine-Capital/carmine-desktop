## MODIFIED Requirements

### Requirement: Secure token storage
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files.

#### Scenario: Token storage on Linux
- **WHEN** tokens are obtained after authentication on Linux
- **THEN** the system stores them via the Secret Service API (GNOME Keyring or KWallet) under the service name "carminedesktop"

#### Scenario: Token storage on macOS
- **WHEN** tokens are obtained after authentication on macOS
- **THEN** the system stores them in the macOS Keychain under the service name "carminedesktop"

#### Scenario: Token storage on Windows
- **WHEN** tokens are obtained after authentication on Windows
- **THEN** the system stores them in Windows Credential Manager under the target "carminedesktop"

#### Scenario: Keychain unavailable fallback
- **WHEN** the OS credential store is unavailable (e.g., no keyring daemon on Linux)
- **THEN** the system stores tokens in an AES-256 encrypted file at the config directory, with the encryption key derived from a user-provided password, and warns the user that this is less secure

### Requirement: Client ID resolution
The system SHALL use the packaged client_id if available, falling back to a built-in default.

#### Scenario: Packaged client_id
- **WHEN** the packaged defaults contain a `[tenant]` section with `client_id`
- **THEN** the OAuth2 flow uses this client_id for all token requests

#### Scenario: No packaged client_id
- **WHEN** no client_id is configured in packaged defaults
- **THEN** the OAuth2 flow uses the built-in carminedesktop default app registration client_id
