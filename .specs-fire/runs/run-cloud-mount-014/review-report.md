# Code Review Report: fix-auth-security

**Run**: run-cloud-mount-014
**Work Item**: fix-auth-security

## Summary

| Category | Auto-Fixed | Suggestions | Skipped |
|----------|-----------|-------------|---------|
| Code Quality | 0 | 0 | 0 |
| Security | 0 | 0 | 0 |
| Architecture | 0 | 0 | 0 |
| Testing | 0 | 0 | 0 |

## Files Reviewed

### `crates/cloudmount-auth/src/storage.rs`

- **Fix 1 (permissions)**: `OpenOptions::mode(0o600)` correctly applied behind `#[cfg(unix)]` with `#[cfg(not(unix))]` fallback to `std::fs::write`. Both branches produce the same error type.
- **Fix 2 (sanitize)**: `sanitize_account_id()` allows `.`, `@`, `-`, `_` and alphanumerics — appropriate for Microsoft account identifiers. Unit test covers edge cases.
- **Fix 4 (machine_id)**: All three platform variants use `Option` return with graceful fallback. Linux reads a file, macOS/Windows shell out to existing system tools. No new dependencies added.
- **Fix 5 (no fallback)**: `encrypted_token_path()` returns `Result<PathBuf>`. All internal callers already propagate via `?`.

### `crates/cloudmount-auth/src/manager.rs`

- **Fix 3 (account_id)**: `AuthState.account_id` added. `storage_key()` helper provides backward-compatible fallback to `client_id`. `try_restore` sets `account_id` in state. `sign_out` reads storage key before clearing state (correct ordering). `set_account_id()` is public async for app use.

### `crates/cloudmount-core/src/config.rs`

- **Fix 5**: `config_dir()` and `config_file_path()` return `Result<PathBuf>`. Error message is descriptive.

### `crates/cloudmount-app/src/commands.rs`

- `set_account_id()` called in `complete_sign_in` after discovering drive identity — correct placement.
- All `config_file_path()` calls now handle `Result` via `?` or `match`.

### `crates/cloudmount-app/src/main.rs`

- Startup `config_file_path()` failure exits with clear error message — appropriate for fatal precondition.
- Non-critical paths use `match` with `tracing::warn!` on failure.

## Conclusion

No issues found. All changes are minimal, correctly platform-gated, and follow project conventions (thiserror errors, tracing logging, `#[cfg]` gates).
