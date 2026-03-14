## MODIFIED Requirements

### Requirement: Secure token storage
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files. The system SHALL verify that tokens were actually persisted after writing to the credential store.

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
