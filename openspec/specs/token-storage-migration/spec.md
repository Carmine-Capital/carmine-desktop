## Requirements

### Requirement: Sign-in finalization migrates token storage key
When a sign-in is finalized and the user's account ID becomes known, the system SHALL migrate any tokens previously stored under the OAuth client ID key to the correct account ID key. The migration SHALL follow a store-then-delete ordering: the tokens MUST be written to the new key before the old key is deleted. If the write to the new key fails, the old key MUST NOT be deleted, preserving the tokens. `AuthManager::finalize_sign_in` SHALL be the single point of responsibility for this migration at sign-in time.

#### Scenario: Tokens stored under client_id after exchange
- **WHEN** `finalize_sign_in(account_id)` is called after a fresh sign-in where `exchange_code` stored tokens under the client_id key
- **THEN** the system SHALL load tokens from the client_id key, store them under the account_id key, and only then delete the client_id key; the in-memory `account_id` SHALL be updated to the new value

#### Scenario: Migration write failure — old key preserved
- **WHEN** `finalize_sign_in(account_id)` is called and the write to the account_id key fails (storage backend error)
- **THEN** the system SHALL NOT delete the client_id key; the error SHALL be propagated to the caller

#### Scenario: No migration needed — keys already match
- **WHEN** `finalize_sign_in(account_id)` is called and the current storage key already equals the account_id
- **THEN** the system SHALL update in-memory state and return without performing any storage operations

### Requirement: Token restore falls back to client_id key for existing installations
When restoring tokens on startup, if no tokens are found under the account ID key, the system SHALL attempt to load tokens stored under the OAuth client ID key as a one-time migration fallback. If tokens are found under the client ID key, they SHALL be migrated to the account ID key (store-then-delete) before the restore proceeds. This ensures that installations that signed in before the sign-in finalization fix was deployed can self-repair on the next restart without requiring the user to re-authenticate.

#### Scenario: Restore succeeds after client_id fallback
- **WHEN** `try_restore(account_id)` is called, no tokens exist under the account_id key, but tokens exist under the client_id key
- **THEN** the system SHALL migrate the tokens (store under account_id, delete under client_id) and proceed with the restored tokens as if they had been found under the account_id key

#### Scenario: Restore fails — no tokens under either key
- **WHEN** `try_restore(account_id)` is called and no tokens exist under the account_id key or the client_id key
- **THEN** the system SHALL return `false` and open the sign-in wizard

#### Scenario: Restore succeeds on second restart — no fallback needed
- **WHEN** `try_restore(account_id)` is called after a previous restart already migrated the tokens
- **THEN** tokens are found under the account_id key directly; the client_id fallback path is not attempted
