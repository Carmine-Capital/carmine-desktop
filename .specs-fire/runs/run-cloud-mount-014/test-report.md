# Test Report: fix-auth-security

**Run**: run-cloud-mount-014
**Work Item**: fix-auth-security

## Test Results

| Crate | Passed | Failed | Ignored |
|-------|--------|--------|---------|
| cloudmount-app (unit) | 6 | 0 | 0 |
| cloudmount-app (integration) | 13 | 0 | 2 |
| cloudmount-auth (unit) | 1 | 0 | 0 |
| cloudmount-auth (integration) | 5 | 0 | 0 |
| cloudmount-core (unit) | 0 | 0 | 0 |
| cloudmount-core (integration) | 11 | 0 | 0 |
| **Total** | **36** | **0** | **2** |

## New Tests Added

- `storage::tests::sanitize_account_id_strips_path_traversal` — validates sanitization of path-traversal characters in account_id for filename use

## Acceptance Criteria Validation

- [x] Encrypted token files created with mode 0600 on Unix — `store_tokens_encrypted` uses `OpenOptions::new().mode(0o600)` behind `#[cfg(unix)]`
- [x] `account_id` is sanitized before use in filenames — `sanitize_account_id()` replaces unsafe chars
- [x] `try_restore` uses the `account_id` parameter, not `client_id` — loads and sets `state.account_id`
- [x] `machine_password()` incorporates platform-specific machine ID — reads `/etc/machine-id` (Linux), `IOPlatformUUID` (macOS), `MachineGuid` (Windows)
- [x] `encrypted_token_path` and `config_dir()` return error instead of falling back to "." — returns `Result<PathBuf>`
- [x] Existing auth tests pass — all 5 integration tests + 1 unit test pass

## Clippy

Zero warnings on `cloudmount-core`, `cloudmount-auth`, `cloudmount-app`.

## Notes

- 2 ignored tests (`test_e2e_*`) require live Graph API — expected
- Pre-existing VFS test failures (from `fix-vfs-data-safety` branch changes) are unrelated
- `machine_password` change invalidates existing `.enc` files — users re-authenticate once (handled gracefully: decryption failure → sign-in flow)
