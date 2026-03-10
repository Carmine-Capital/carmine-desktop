## 1. AuthManager — `finalize_sign_in`

- [x] 1.1 Add `pub async fn finalize_sign_in(&self, id: &str) -> cloudmount_core::Result<()>` to `AuthManager` in `crates/cloudmount-auth/src/manager.rs`
- [x] 1.2 In `finalize_sign_in`: capture the current storage key via `storage_key()` before updating `account_id` in state
- [x] 1.3 In `finalize_sign_in`: update `state.account_id` to `Some(id.to_string())`
- [x] 1.4 In `finalize_sign_in`: if `old_key != id`, call `storage::load_tokens(&old_key)`; if tokens found, call `storage::store_tokens(id, &tokens)` and only on success call `storage::delete_tokens(&old_key)`; propagate any store error
- [x] 1.5 In `finalize_sign_in`: if `old_key == id`, return `Ok(())` immediately (no-op path)

## 2. AuthManager — `try_restore` fallback

- [x] 2.1 In `try_restore` in `crates/cloudmount-auth/src/manager.rs`: after `load_tokens(account_id)` returns `None`, attempt `storage::load_tokens(&self.client_id)`
- [x] 2.2 If tokens found under `client_id`: call `storage::store_tokens(account_id, &tokens)`; on success call `storage::delete_tokens(&self.client_id)`; log a `tracing::info!` message indicating migration occurred; continue restore with the migrated tokens
- [x] 2.3 If tokens not found under `client_id` either: return `Ok(false)` (existing behavior, no change)

## 3. App — wire `finalize_sign_in`

- [x] 3.1 In `complete_sign_in` in `crates/cloudmount-app/src/commands.rs` (line 130): replace `state.auth.set_account_id(&drive.id).await` with `state.auth.finalize_sign_in(&drive.id).await.map_err(|e| e.to_string())?`

## 4. Tests

- [x] 4.1 Add test `finalize_sign_in_migrates_tokens_from_client_id_to_account_id` in `crates/cloudmount-auth/tests/`: store tokens under client_id, call `finalize_sign_in(account_id)`, assert tokens exist under account_id and not under client_id
- [x] 4.2 Add test `finalize_sign_in_noop_when_key_already_correct`: set account_id first, call `finalize_sign_in` with same id, assert no storage changes
- [x] 4.3 Add test `try_restore_falls_back_to_client_id_and_migrates`: store tokens under client_id only, call `try_restore(account_id)`, assert returns true and tokens are now under account_id not client_id
- [x] 4.4 Add test `try_restore_succeeds_directly_when_tokens_under_account_id`: store under account_id, call `try_restore(account_id)`, assert returns true and client_id fallback was not needed (client_id key is empty)

## 5. CI

- [x] 5.1 Run `make clippy` and resolve any warnings
- [x] 5.2 Run `make test` and confirm all tests pass
