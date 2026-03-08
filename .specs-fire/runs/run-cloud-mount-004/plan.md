# Implementation Plan — clear-mounts-on-sign-out

**Run:** run-cloud-mount-004
**Mode:** autopilot
**Work Item:** Clear mounts list in sign_out command

## Approach

Single-line addition in `commands.rs`. After `user_config.accounts.clear()` in the `sign_out`
function, add `user_config.mounts.clear()` so the saved config has an empty mounts array.
`rebuild_effective_config` is already called immediately after, propagating the cleared state.

## Files to Create

(none)

## Files to Modify

- `crates/cloudmount-app/src/commands.rs` — add `user_config.mounts.clear()` after accounts.clear() in sign_out

## Tests

- `cargo clippy --all-targets --all-features` — zero warnings
- `cargo fmt --all -- --check` — formatting
- `cargo test -p cloudmount-app` — existing tests must pass
