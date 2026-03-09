---
id: fix-auth-security
title: Harden auth (token perms, account_id, try_restore, machine_password)
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-014
completed_at: 2026-03-09T19:19:35.757Z
---

# Work Item: Harden auth (token perms, account_id, try_restore, machine_password)

## Description

Fix security and correctness issues in the auth crate:

1. **Token file world-readable** (`storage.rs:184`): `std::fs::write` uses default umask (typically 0644). Fix: on Unix, use `OpenOptions::new().mode(0o600)` via `std::os::unix::fs::OpenOptionsExt`. Windows `%APPDATA%` default ACL is already user-only.

2. **account_id unsanitized in filename** (`storage.rs:137`): `format!("tokens_{account_id}.enc")` used directly as path component. If account_id contains `/` or `\`, path traversal is possible. Fix: sanitize by replacing non-alphanumeric chars (except `-`) with `_`, or use a hash.

3. **try_restore ignores account_id** (`manager.rs:57`): Parameter is `_account_id` (unused). Uses `self.client_id` instead. Fix: pass `account_id` to `storage::load_tokens()`. This is a blocker for future multi-account support.

4. **machine_password deterministic** (`storage.rs:152-160`): Built from `USER`/`USERNAME` env + `config_dir()` — trivially guessable. Fix: incorporate machine-specific entropy (e.g., `/etc/machine-id` on Linux, `IOPlatformUUID` on macOS, `MachineGuid` registry key on Windows).

5. **Config dir fallback to "."** (`storage.rs:135`, `config.rs:370`): If `dirs::config_dir()` returns `None`, falls back to current directory. Fix: return an error instead of silently using `.`.

## Acceptance Criteria

- [ ] Encrypted token files created with mode 0600 on Unix
- [ ] `account_id` is sanitized before use in filenames
- [ ] `try_restore` uses the `account_id` parameter, not `client_id`
- [ ] `machine_password()` incorporates platform-specific machine ID
- [ ] `encrypted_token_path` and `config_dir()` return error instead of falling back to "."
- [ ] Existing auth tests pass

## Technical Notes

For machine_password, read `/etc/machine-id` (Linux), run `ioreg -rd1 -c IOPlatformExpertDevice` (macOS), or read `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` (Windows). Use `#[cfg]` gates per platform. Fall back to current behavior if machine ID is unavailable.

## Dependencies

(none)
