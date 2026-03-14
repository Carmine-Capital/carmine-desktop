## MODIFIED Requirements

### Requirement: Secure token storage
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files. The system SHALL verify that tokens were actually persisted after writing to the credential store. The storage key used for storing, loading, refreshing, and deleting tokens SHALL be the application client_id — consistent across all token lifecycle operations.

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

#### Scenario: Consistent storage key across token operations
- **WHEN** the system stores, loads, refreshes, or deletes tokens in the credential store or encrypted file fallback
- **THEN** all operations SHALL use the application client_id as the credential key; the token restoration method (`try_restore`) SHALL NOT use a caller-provided account identifier as the storage lookup key, as this would cause a key mismatch with the store operation which uses the client_id
