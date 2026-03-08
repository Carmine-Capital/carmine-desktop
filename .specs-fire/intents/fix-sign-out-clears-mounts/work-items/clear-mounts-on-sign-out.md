---
id: clear-mounts-on-sign-out
title: Clear mounts list in sign_out command
intent: fix-sign-out-clears-mounts
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-004
completed_at: 2026-03-08T13:34:49.509Z
---

# Work Item: Clear mounts list in sign_out command

## Description

In `crates/cloudmount-app/src/commands.rs`, the `sign_out` function clears `accounts` but not `mounts`. Add `user_config.mounts.clear()` so that sign-out produces a fully clean config, preventing stale drive IDs from being re-attempted on reconnect (same or different account).

## Acceptance Criteria

- [ ] `user_config.mounts.clear()` called inside `sign_out` before `save_to_file`
- [ ] Saved config file contains empty `mounts` array after sign-out
- [ ] Reconnecting after sign-out starts with no pre-existing mounts (wizard shows fresh)
- [ ] Account switch (A→B) no longer attempts A's drive IDs with B's token
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo fmt --all -- --check` passes

## Technical Notes

In `commands.rs:sign_out` (around line 179-191), inside the `match state.user_config.lock()` block:

```rust
Ok(mut user_config) => {
    user_config.accounts.clear();
    user_config.mounts.clear();  // ADD THIS LINE
    if let Err(e) = user_config.save_to_file(&config_file_path()) {
        ...
    }
}
```

No other changes needed. `rebuild_effective_config` is already called after, which will propagate the empty mounts to `effective_config`.

## Dependencies

(none)
