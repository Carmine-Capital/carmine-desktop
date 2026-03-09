---
run: run-cloud-mount-014
work_item: fix-auth-security
intent: fix-comprehensive-review
generated: 2026-03-09T19:20:00Z
mode: confirm
---

# Implementation Walkthrough: Harden auth (token perms, account_id, try_restore, machine_password)

## Summary

Five security fixes applied to the auth and config subsystems: restrictive file permissions on encrypted token files (Unix 0600), path-traversal prevention via account_id sanitization, correct account_id usage throughout the token lifecycle, platform-specific machine ID entropy for key derivation, and error propagation instead of silent fallback to current directory when no config directory is available.

## Structure Overview

The auth crate's `storage.rs` handles encrypted token file I/O with AES-256-GCM. The `manager.rs` orchestrates the token lifecycle (store, load, refresh, delete) and now tracks which account owns the tokens via `AuthState.account_id`. The app's `commands.rs` bridges the two by calling `set_account_id()` after discovering the user identity from the Graph API. The core crate's `config.rs` provides `config_dir()` and `config_file_path()` which now return errors instead of silently falling back to `"."`.

## Files Changed

### Created

None.

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-auth/src/storage.rs` | File permissions (0600), account_id sanitization, machine_id platform entropy, encrypted_token_path returns Result |
| `crates/cloudmount-auth/src/manager.rs` | `account_id` in AuthState, `try_restore` uses parameter, `storage_key()` helper, `set_account_id()` method |
| `crates/cloudmount-core/src/config.rs` | `config_dir()` and `config_file_path()` return `Result<PathBuf>` |
| `crates/cloudmount-app/src/commands.rs` | `set_account_id()` call in `complete_sign_in`, all `config_file_path()` calls handle Result |
| `crates/cloudmount-app/src/main.rs` | Startup fatal error on missing config dir, runtime `config_file_path()` Result handling |

## Key Implementation Details

### 1. Token file permissions (Fix 1)

On Unix, `store_tokens_encrypted()` uses `OpenOptions::new().mode(0o600)` behind `#[cfg(unix)]` to create token files readable only by the owner. A `#[cfg(not(unix))]` branch keeps the existing `std::fs::write` for Windows where `%APPDATA%` ACLs are already user-only.

### 2. Account ID sanitization (Fix 2)

`sanitize_account_id()` replaces any character not in `[a-zA-Z0-9\-_\.@]` with `_`. This prevents path traversal via crafted account identifiers (e.g., `../../etc/passwd` → `.._.._etc_passwd`).

### 3. Account ID lifecycle (Fix 3)

`AuthState` gained an `account_id: Option<String>` field. `try_restore(account_id)` sets it and loads tokens by `account_id` (not `client_id`). A `storage_key()` helper returns `account_id` if set, else falls back to `client_id` for backward compatibility. All token store/delete operations use this helper. The app calls `set_account_id()` in `complete_sign_in` after discovering the user identity from the Graph API.

### 4. Machine-specific entropy (Fix 4)

`machine_id()` reads platform-specific identifiers: `/etc/machine-id` on Linux, `IOPlatformUUID` on macOS (via `ioreg`), `MachineGuid` on Windows (via `reg query`). Falls back to empty string if unavailable. This entropy is appended to the KDF password, making brute-force harder.

### 5. Config dir error propagation (Fix 5)

`config_dir()` and `config_file_path()` now return `Result<PathBuf>`. Callers in the app crate handle the Result: startup fatal-exits, runtime operations log warnings or propagate errors to the UI.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Backward compat for storage_key | Fallback to `client_id` when `account_id` not set | Preserves first sign-in flow where account_id is unknown until Graph API responds |
| machine_id via shell commands (macOS/Windows) | `ioreg` / `reg query` | No new dependencies needed; system tools are always available |
| Allow `.` in sanitized account_id | Keep dots, strip slashes | Microsoft account IDs can contain dots; dots don't enable path traversal |
| Fatal exit on missing config dir at startup | `eprintln!` + `exit(1)` | App cannot function without config; better to fail fast with clear message |

## Deviations from Plan

None. All five fixes implemented as planned.

## Dependencies Added

None. All fixes use standard library and existing dependencies.

## How to Verify

1. **Build and test**
   ```bash
   toolbox run -c cloudmount-build cargo build -p cloudmount-core -p cloudmount-auth -p cloudmount-app
   toolbox run -c cloudmount-build cargo test -p cloudmount-auth -p cloudmount-core -p cloudmount-app
   ```
   Expected: clean build, 36 tests pass

2. **Verify file permissions (manual, after sign-in)**
   ```bash
   ls -la ~/.config/cloudmount/tokens_*.enc
   ```
   Expected: `-rw-------` (0600) on newly created files

3. **Verify clippy**
   ```bash
   toolbox run -c cloudmount-build cargo clippy -p cloudmount-core -p cloudmount-auth -p cloudmount-app
   ```
   Expected: zero warnings

## Test Coverage

- Tests added: 1 (sanitize_account_id unit test)
- Existing tests passing: 36
- Status: all passing

## Developer Notes

- **Breaking change**: `machine_password()` now includes machine ID, so existing `.enc` files won't decrypt. Users will be prompted to re-authenticate once. The auth flow handles this gracefully — decryption failure returns `None`, triggering the sign-in flow.
- **Migration**: If you need to preserve existing encrypted tokens during testing, temporarily revert the `machine_password()` change.
- **`config_dir()` signature change**: Any new code calling `config_dir()` or `config_file_path()` must handle the `Result`. This is the correct API — silent fallback to `"."` was the bug.

---
*Generated by specs.md - fabriqa.ai FIRE Flow Run run-cloud-mount-014*
