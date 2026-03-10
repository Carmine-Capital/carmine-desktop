## MODIFIED Requirements

### Requirement: Secure token storage
The system SHALL store OAuth tokens in the operating system's native secure credential store. Tokens MUST NOT be stored in plaintext configuration files. The system SHALL verify that tokens were actually persisted after writing to the credential store. Tokens SHALL be stored and loaded using the user's account ID (drive ID) as the storage key. Immediately after token exchange (before the account ID is known), tokens are transiently stored under the OAuth client ID key; `AuthManager::finalize_sign_in` MUST be called once the account ID is known to migrate tokens to the permanent account ID key.

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

#### Scenario: Permanent storage key is account ID after finalization
- **WHEN** `finalize_sign_in(account_id)` has been called after a successful sign-in
- **THEN** all subsequent token operations (load, refresh, delete) SHALL use the account ID as the storage key; the OAuth client ID SHALL NOT be used as a storage key after finalization
