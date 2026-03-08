---
id: filter-mounts-by-account-on-sign-in
title: Filter mounts by account_id on sign-in and sign-out
intent: feat-account-scoped-mounts
complexity: medium
mode: confirm
status: completed
depends_on:
  - add-account-id-to-mount-config
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-006
completed_at: 2026-03-08T14:23:11.793Z
---

# Work Item: Filter mounts by account_id on sign-in and sign-out

## Description

Change sign-out to preserve mounts in config (revert the `mounts.clear()` from `fix-sign-out-clears-mounts`), and change `start_all_mounts` to only start mounts where `account_id == current_account_id`. This allows same-account config persistence while isolating accounts from each other.

## Acceptance Criteria

- [ ] `sign_out` no longer clears `mounts` (accounts are still cleared)
- [ ] `start_all_mounts` filters mounts: only starts those with `account_id` matching the authenticated drive ID (or `None` if no `account_id` — treat as compatible for migration)
- [ ] Account switch (A→B): B's reconnect does not start A's mounts
- [ ] Same-account reconnect: all of A's mounts are restored without reconfiguration
- [ ] Mounts with `account_id: None` (legacy configs) are skipped on reconnect with a log warning
- [ ] `cargo test -p cloudmount-app` passes
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings

## Technical Notes

- `start_all_mounts` in `main.rs` receives the `effective_config`; filtering can be done there or in `rebuild_effective_config` by stripping non-matching mounts from the effective view
- The authenticated account ID is available in `AppState` after `complete_sign_in` (store it in a new `AppState.account_id: Mutex<Option<String>>` field, set during `complete_sign_in`)
- Filtering in `rebuild_effective_config` is cleaner: `EffectiveConfig` only contains mounts for the current account

## Dependencies

- add-account-id-to-mount-config
