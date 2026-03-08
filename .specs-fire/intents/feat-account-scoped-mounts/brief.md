---
id: feat-account-scoped-mounts
title: Account-scoped mount configuration
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T14:23:11.801Z
---

# Intent: Account-scoped mount configuration

## Goal

Tag each mount with the `account_id` of the user who created it, so mounts persist across sign-out/sign-in for the same account while remaining invisible to different accounts.

## Users

Users who sign out and back in with the same account and want their mount configuration preserved without reconfiguring everything each time.

## Problem

After `fix-sign-out-clears-mounts`, mount config is cleared on every sign-out — including for same-account reconnects. A power user with 10 SharePoint libraries configured who signs out due to a network issue or token expiry loses all configuration. The `fix-sign-out-clears-mounts` intent trades config loss for safety (cross-account stale mounts). This intent restores the convenience without the safety risk.

## Success Criteria

- Each mount in config has an `account_id` field (set to the owning drive/user ID at creation)
- On sign-out: mounts are preserved in config (not cleared)
- On sign-in: only mounts matching `account_id == authenticated_account_id` are started
- Account switch (A→B): B sees no mounts from A; B's own mounts (if any) are restored
- Config file migration: existing mounts without `account_id` are handled gracefully (skip or prompt)
- CI passes (clippy + fmt + config tests)

## Constraints

- Config schema change: add `account_id: String` to mount struct in `cloudmount-core/src/config.rs`
- Must be backward-compatible: existing configs without `account_id` should deserialize without error (use `Option<String>`)
- `start_all_mounts` in `main.rs` must filter by account
- Depends on: `fix-sign-out-clears-mounts` deployed first (so existing users start clean)

## Notes

This is Layer 1 of the ultimate solution. Does not address deleted/revoked resources (that is `feat-mount-validation`). Account ID should be the authenticated drive ID (`drive.id` from `get_my_drive()`) for consistency with what's already stored at sign-in.
