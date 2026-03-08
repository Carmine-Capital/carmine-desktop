# Test Report — run-cloud-mount-006 / WI-1

## Work Item: `add-account-id-to-mount-config`

## Test Run Summary

| Suite | Passed | Failed | Ignored |
|-------|--------|--------|---------|
| `cloudmount-core` (config_tests) | 11 | 0 | 0 |
| `cloudmount-app` (unit + integration) | 18 | 0 | 2 |

All 29 tests pass. 2 ignored tests require live Graph API (expected).

## Acceptance Criteria Validation

- [x] `MountConfig.account_id: Option<String>` with `#[serde(default)]` — already existed, verified
- [x] `add_onedrive_mount` accepts and stores `account_id` — added param, threaded through
- [x] `add_sharepoint_mount` accepts and stores `account_id` — added param, threaded through
- [x] `commands.rs:add_mount` passes current drive account id — reads from `user_config.accounts.first()`
- [x] Existing TOML without `account_id` deserializes as `None` — `test_mount_config_account_id_none_compat`
- [x] `cargo test -p cloudmount-core` passes — 11/11
- [x] `cargo clippy` passes with zero new warnings — 3 pre-existing warnings in unmodified code

## New Tests Added

| Test | File | Verifies |
|------|------|---------|
| `test_mount_config_account_id_stored` | `config_tests.rs` | `add_onedrive_mount` stores `account_id`, survives TOML round-trip |
| `test_mount_config_account_id_none_compat` | `config_tests.rs` | TOML without `account_id` deserializes as `None` |

## Notes

- Pre-existing clippy warnings (not introduced by this work item): `collapsible_if` at commands.rs:296, `type_complexity` at main.rs:105 and main.rs:879.
