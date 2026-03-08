# Implementation Plan — run-cloud-mount-006

## Intent: Account-scoped mount configuration

---

## Work Item 1: `add-account-id-to-mount-config`

### Current State
- `MountConfig` already has `account_id: Option<String>` with `#[serde(default)]` ✅
- `add_sharepoint_mount` and `add_onedrive_mount` both hardcode `account_id: None` ❌
- `commands.rs:add_mount` never passes an account_id through ❌

### Approach
Add an `account_id: Option<String>` parameter to both `add_sharepoint_mount` and `add_onedrive_mount` in `config.rs`, then thread the value through from all callers.

In `commands.rs:add_mount` (sync fn), retrieve the current account_id from already-loaded metadata: `user_config.accounts.first().map(|a| a.id.clone())` — no async needed.

### Files to Modify

| File | Change |
|------|--------|
| `crates/cloudmount-core/src/config.rs` | Add `account_id: Option<String>` param to `add_sharepoint_mount` and `add_onedrive_mount` |
| `crates/cloudmount-app/src/commands.rs` | Read account_id from `user_config.accounts.first()`, pass to mount creation |
| `crates/cloudmount-app/src/main.rs` | Update headless `add_onedrive_mount` call at line 1124 to pass `Some(drive.id.clone())` |
| `crates/cloudmount-app/tests/integration_tests.rs` | Update 2 `add_onedrive_mount` call sites to pass `None` |
| `crates/cloudmount-core/tests/config_tests.rs` | Add test: `account_id` is stored and survives round-trip |

### Tests
- `test_mount_config_account_id_stored` — verify `add_onedrive_mount` with `Some("acc-1")` persists `account_id`
- `test_mount_config_account_id_none_compat` — verify TOML without `account_id` deserializes as `None`

---

## Work Item 2: `filter-mounts-by-account-on-sign-in`

### Approach
1. Add `account_id: Mutex<Option<String>>` to `AppState` — set to `drive.id` on sign-in, cleared on sign-out
2. Remove `user_config.mounts.clear()` from `sign_out` (accounts still cleared)
3. Modify `rebuild_effective_config` to filter `EffectiveConfig.mounts` to only mounts matching the current `account_id`; mounts with `account_id: None` are skipped with a `tracing::warn!`
4. On sign-out → `account_id` set to `None` → `rebuild_effective_config` produces empty mounts → no mounts start on next sign-in to a different account

### Files to Modify

| File | Change |
|------|--------|
| `crates/cloudmount-app/src/main.rs` | Add `account_id: Mutex<Option<String>>` to `AppState`, initialize as `None` |
| `crates/cloudmount-app/src/commands.rs` | `complete_sign_in`: set `state.account_id` to `drive.id`; `sign_out`: clear `account_id`, remove `mounts.clear()`; `rebuild_effective_config`: filter mounts by account_id |
| `crates/cloudmount-app/tests/integration_tests.rs` | Update `test_sign_out_clears_account_and_config` to reflect mounts preserved; add test for account-filtered mounts |

### Tests (new)
- `test_account_scoped_mounts_filtered` — verify EffectiveConfig only contains mounts for matching `account_id`
- `test_sign_out_preserves_mounts_clears_account` — verify sign-out keeps mounts in config but clears accounts; `EffectiveConfig` shows empty mounts after (no account_id set)

---

## Execution Order

1. WI-1: config.rs + callers → tests pass → complete item
2. WI-2: AppState + commands → test updates → complete run

## No New Dependencies Required
