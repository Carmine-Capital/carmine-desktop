# Implementation Plan: fix-auth-security

**Run**: run-cloud-mount-014
**Work Item**: fix-auth-security
**Mode**: confirm
**Intent**: fix-comprehensive-review

## Approach

Five targeted security fixes in the auth crate and config module. Each fix is isolated and testable independently. Changes are confined to `storage.rs`, `manager.rs`, and `config.rs`.

## Fix 1: Token file permissions (storage.rs:184)

**Problem**: `std::fs::write()` uses default umask (typically 0644), making encrypted token files world-readable.

**Fix**: On Unix, use `std::fs::OpenOptions` with `.mode(0o600)` via `std::os::unix::fs::OpenOptionsExt`. On Windows, `%APPDATA%` default ACL is already user-only — no change needed.

**Files to Modify**:
- `crates/cloudmount-auth/src/storage.rs` — `store_tokens_encrypted()`, replace `std::fs::write` with `OpenOptions` + `#[cfg(unix)]` mode

## Fix 2: Sanitize account_id in filename (storage.rs:137)

**Problem**: `format!("tokens_{account_id}.enc")` could allow path traversal if `account_id` contains `/` or `\`.

**Fix**: Add `sanitize_account_id()` helper that replaces non-alphanumeric chars (except `-`, `_`, `.`, `@`) with `_`. Apply in `encrypted_token_path()`.

**Files to Modify**:
- `crates/cloudmount-auth/src/storage.rs` — add `sanitize_account_id()`, use in `encrypted_token_path()`

## Fix 3: try_restore uses account_id (manager.rs:57)

**Problem**: `try_restore(_account_id)` ignores the parameter and loads tokens using `self.client_id`.

**Fix**:
- Add `account_id: Option<String>` to `AuthState`
- `try_restore(account_id)`: load tokens by `account_id`, store it in state
- `exchange_code`/`refresh`: store tokens under `state.account_id` if set, else `self.client_id`
- `sign_out`: delete tokens under `state.account_id` if set, else `self.client_id`; clear `account_id`
- Add public `set_account_id(&self, id: &str)` so the app can set it after first sign-in (when account_id is discovered from Graph API)

**Files to Modify**:
- `crates/cloudmount-auth/src/manager.rs` — `AuthState`, `try_restore`, `exchange_code`, `refresh`, `sign_out`, new `set_account_id`
- `crates/cloudmount-app/src/commands.rs` — call `auth.set_account_id()` in `complete_sign_in`

## Fix 4: machine_password with platform entropy (storage.rs:152-160)

**Problem**: Machine password built from `USER` env + `config_dir()` is trivially guessable.

**Fix**: Incorporate platform-specific machine ID via `#[cfg]` gates:
- **Linux**: Read `/etc/machine-id`
- **macOS**: Run `ioreg -rd1 -c IOPlatformExpertDevice`, parse `IOPlatformUUID`
- **Windows**: Read `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` via `reg query`
- **Fallback**: Current behavior if machine ID unavailable (graceful degradation)

**Files to Modify**:
- `crates/cloudmount-auth/src/storage.rs` — `machine_password()`, add platform-specific `machine_id()` helpers

**Note**: Existing encrypted token files will fail to decrypt after this change since the key derivation password changes. This is acceptable — users will need to re-authenticate once, and the token refresh flow handles this gracefully (decryption failure → return `None` → trigger sign-in).

## Fix 5: Config dir error instead of "." fallback (storage.rs:135, config.rs:370)

**Problem**: If `dirs::config_dir()` returns `None`, code falls back to current directory, which is unpredictable and insecure.

**Fix**:
- `encrypted_token_path()`: Return `Err(Error::Auth(...))` if config dir unavailable
- `config_dir()` in config.rs: Return `Result<PathBuf>` instead of `PathBuf`, propagate error
- Update callers of `config_dir()` and `config_file_path()` to handle `Result`

**Files to Modify**:
- `crates/cloudmount-auth/src/storage.rs` — `encrypted_token_path()` returns `Result<PathBuf>`
- `crates/cloudmount-core/src/config.rs` — `config_dir()` and `config_file_path()` return `Result<PathBuf>`
- Callers of `config_dir()` / `config_file_path()` across crates

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-auth/src/storage.rs` | Fix 1 (perms), Fix 2 (sanitize), Fix 4 (machine ID), Fix 5 (no "." fallback) |
| `crates/cloudmount-auth/src/manager.rs` | Fix 3 (account_id in state, try_restore, store/delete) |
| `crates/cloudmount-core/src/config.rs` | Fix 5 (config_dir returns Result) |
| `crates/cloudmount-app/src/commands.rs` | Fix 3 (set_account_id after sign-in) |
| `crates/cloudmount-app/src/main.rs` | Fix 5 (handle config_dir Result) |

## Tests

- Existing `auth_integration.rs` tests must continue to pass
- Add test for `sanitize_account_id` with path-traversal inputs
- Verify `encrypted_token_path` returns error when no config dir

## Risk Assessment

- **Fix 4 (machine_password)**: Changing KDF input invalidates existing `.enc` files. Users re-authenticate once. Acceptable.
- **Fix 5 (config_dir)**: Signature change from `PathBuf` → `Result<PathBuf>` cascades to callers. Scoped, but needs careful caller updates.
- **Fix 3 (account_id)**: Backward-compatible via fallback to `client_id` when `account_id` not set.
