## Why

After a successful sign-in, tokens are stored under the OAuth client ID because the user's account ID (drive ID) is not yet known at the time of the token exchange. When the app restarts, it attempts to restore tokens using the account ID — which doesn't match the stored key — causing the restore to fail silently and forcing the user to re-authenticate every time they relaunch the app.

## What Changes

- Add `AuthManager::finalize_sign_in(id: &str)` method that sets the account ID and migrates any tokens previously stored under the client ID key to the correct account ID key, atomically (store new → delete old, only on success).
- Modify `AuthManager::try_restore` to fall back to the client ID key if no tokens are found under the account ID key, and auto-migrate found tokens to the correct key. This repairs existing broken installations.
- Replace the call to `set_account_id` in `complete_sign_in` with `finalize_sign_in`.

## Capabilities

### New Capabilities

- `token-storage-migration`: Mechanism to migrate persisted tokens from a temporary storage key (client ID) to the correct permanent key (account ID) transparently at sign-in finalization and at restore time.

### Modified Capabilities

- `microsoft-auth`: The token persistence and restore contract changes — restore now includes a client-ID fallback migration path, and sign-in finalization now guarantees tokens are stored under the account ID key before returning.

## Impact

- `crates/cloudmount-auth/src/manager.rs`: new `finalize_sign_in` method, modified `try_restore`
- `crates/cloudmount-app/src/commands.rs`: one-line change in `complete_sign_in` (`set_account_id` → `finalize_sign_in`)
- No public API changes visible outside `cloudmount-auth`
- No storage format changes — same keyring/encrypted-file backends, same key derivation
- Fixes all existing Linux AppImage installations where tokens are stranded under the client ID key
