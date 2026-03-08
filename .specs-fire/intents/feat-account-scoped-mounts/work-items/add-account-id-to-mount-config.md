---
id: add-account-id-to-mount-config
title: Add account_id field to mount config schema
intent: feat-account-scoped-mounts
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-006
completed_at: 2026-03-08T14:20:42.192Z
---

# Work Item: Add account_id field to mount config schema

## Description

Add an `account_id: Option<String>` field to the mount configuration struct in `cloudmount-core/src/config.rs`. Populate it at mount creation time in `commands.rs:add_mount` using the authenticated drive ID. Existing configs without this field deserialize gracefully (as `None`).

## Acceptance Criteria

- [ ] `MountConfig` struct has `account_id: Option<String>` with `#[serde(default)]`
- [ ] `add_onedrive_mount` and `add_sharepoint_mount` in `config.rs` accept and store `account_id`
- [ ] `commands.rs:add_mount` passes the current drive ID as `account_id` (retrieved from `state.graph.get_my_drive()` or from already-loaded account metadata)
- [ ] Existing TOML configs without `account_id` deserialize without error (`None`)
- [ ] `cargo test -p cloudmount-core` passes (update config tests if needed)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings

## Technical Notes

- `MountConfig` is in `cloudmount-core/src/config.rs`
- Use `#[serde(default)]` on the field so existing configs without it parse as `None`
- The account ID to use is `drive.id` from `get_my_drive()` — already available in `complete_sign_in` and accessible via `state.graph`
- `add_mount` in `commands.rs` is async-capable but currently `fn` — may need a small refactor or pre-fetch the drive ID from stored account metadata to avoid making it async

## Dependencies

(none — can be done independently, but intended to follow `fix-sign-out-clears-mounts`)
