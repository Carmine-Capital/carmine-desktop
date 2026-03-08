# Walkthrough — run-cloud-mount-006

**Intent:** Account-scoped mount configuration
**Scope:** wide (2 work items)
**Duration:** 2026-03-08T14:06:57Z → 2026-03-08T14:23:11Z

---

## Overview

This run implements account-scoped mount configuration: mounts are associated with a specific Microsoft account (via `account_id`), and only mounts belonging to the currently signed-in account are active. When the user signs out and back in as a different account, they see only their own mounts.

---

## WI-1: `add-account-id-to-mount-config`

### What changed

**`crates/cloudmount-core/src/config.rs`** — Both `add_onedrive_mount` and `add_sharepoint_mount` gained an `account_id: Option<String>` parameter (previously hardcoded to `None`). The field already existed on `MountConfig` with `#[serde(default)]`; this wires up the population path.

**`crates/cloudmount-app/src/commands.rs` (`add_mount`)** — Before branching on mount type, reads the current account's ID:
```rust
let account_id = user_config.accounts.first().map(|a| a.id.clone());
```
Passes it into both `add_onedrive_mount` and `add_sharepoint_mount`.

**`crates/cloudmount-app/src/main.rs` (headless auto-mount)** — The headless path that auto-creates a default OneDrive mount now passes `Some(drive.id.clone())` instead of having the field default to `None`.

**`crates/cloudmount-app/tests/integration_tests.rs`** — Two existing call sites updated to pass `None` (tests don't need an account context).

**`crates/cloudmount-core/tests/config_tests.rs`** — Two new tests:
- `test_mount_config_account_id_stored`: verifies `add_onedrive_mount` stores `account_id` and it survives a TOML round-trip.
- `test_mount_config_account_id_none_compat`: verifies existing TOML without `account_id` deserializes as `None`.

### Why this approach

Reading `user_config.accounts.first()` in `add_mount` avoids making the command async. The drive ID was already stored in `AccountMetadata.id` during `complete_sign_in`, so no extra Graph API call is needed.

---

## WI-2: `filter-mounts-by-account-on-sign-in`

### What changed

**`crates/cloudmount-app/src/main.rs`** — Added `account_id: Mutex<Option<String>>` to `AppState` (initialized to `None`). This tracks the currently signed-in account's drive ID independent of user config.

**`crates/cloudmount-app/src/commands.rs`** — Three changes:

1. **`complete_sign_in`** — After saving user config, sets the active account:
   ```rust
   *state.account_id.lock()...? = Some(drive.id.clone());
   ```

2. **`sign_out`** — Clears `account_id` and removes `user_config.mounts.clear()`. Mounts are now preserved in the config file across sign-outs; only accounts are cleared:
   ```rust
   *state.account_id.lock().unwrap() = None;
   // user_config.mounts.clear() removed
   ```

3. **`rebuild_effective_config`** — Now filters `EffectiveConfig.mounts` based on `state.account_id`:
   - If `account_id` is `Some(id)`: only mounts with matching `account_id` are included; mismatched mounts get a `tracing::warn!`.
   - If `account_id` is `None` (signed out): returns empty mounts.

**`crates/cloudmount-app/tests/integration_tests.rs`** — Two changes:
- `test_sign_out_clears_account_and_config`: removed `user_config.mounts.clear()` from the simulation; asserts mounts are preserved (1 mount remains).
- Added `test_account_scoped_mounts_filtered`: verifies that filtering by `account_id` on a `UserConfig` with two mounts for different accounts returns only the matching mount.

### Why this approach

Keeping mounts in the config file across sign-outs means the user doesn't have to re-add their mount points every time they sign in again. The filtering in `rebuild_effective_config` ensures only the right mounts are active at runtime, while the disk config is the authoritative "what the user configured" store.

The `Mutex<Option<String>>` on `AppState` (not derived from `user_config`) gives a clear, runtime-only signal of the signed-in account without reparsing config.

---

## Test Results

| Suite | Passed | Failed |
|-------|--------|--------|
| `cloudmount-core` | 11 | 0 |
| `cloudmount-app` (unit) | 6 | 0 |
| `cloudmount-app` (integration) | 13 | 0 |

**Tests added this run:** `test_mount_config_account_id_stored`, `test_mount_config_account_id_none_compat`, `test_account_scoped_mounts_filtered`

---

## Files Changed

| File | Type | Change |
|------|------|--------|
| `crates/cloudmount-core/src/config.rs` | Modified | `account_id` param on `add_onedrive_mount` + `add_sharepoint_mount` |
| `crates/cloudmount-app/src/commands.rs` | Modified | Set/clear `account_id` in sign-in/out; filter mounts in `rebuild_effective_config` |
| `crates/cloudmount-app/src/main.rs` | Modified | `account_id` field on `AppState`; headless mount creation passes drive ID |
| `crates/cloudmount-app/tests/integration_tests.rs` | Modified | Updated sign-out test; added account-filtering test |
| `crates/cloudmount-core/tests/config_tests.rs` | Modified | Added 2 `account_id` tests |
