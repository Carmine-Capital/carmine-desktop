# Walkthrough — clear-mounts-on-sign-out

**Run:** run-cloud-mount-004
**Completed:** 2026-03-08T13:34:49Z

## What Was Done

Fixed `sign_out` in `commands.rs` to clear the mounts list when a user signs out, preventing
stale drive IDs from being re-attempted on reconnect with the same or a different account.

## The Problem

The `sign_out` command called `user_config.accounts.clear()` but not `user_config.mounts.clear()`.
This meant that after sign-out, the saved `config.toml` still contained the previous mounts. On
the next sign-in, `start_all_mounts` would try to mount those drives with the new account's token,
which would fail silently or behave unexpectedly (especially on account switch A→B).

## The Fix

**`crates/cloudmount-app/src/commands.rs`** — one line added:

```rust
// Before
user_config.accounts.clear();
if let Err(e) = user_config.save_to_file(...) { ... }

// After
user_config.accounts.clear();
user_config.mounts.clear();  // ← NEW
if let Err(e) = user_config.save_to_file(...) { ... }
```

The existing `rebuild_effective_config` call immediately following the lock block propagates the
empty mounts to `effective_config` — no other changes needed.

## Test Update

**`crates/cloudmount-app/tests/integration_tests.rs`** — `test_sign_out_clears_account_and_config`:

- Added `user_config.mounts.clear()` to the sign-out simulation (steps 1-3)
- Changed the assertion from `reloaded.mounts.len() == 1` (old behavior) to
  `reloaded.mounts.is_empty()` (new behavior)
- The comment "Mounts remain in config (they aren't deleted on sign-out, just stopped)" was removed
  as it no longer reflects the intended behavior

## Verification

```
cargo fmt --all -- --check        → PASS
cargo clippy -p cloudmount-app    → PASS, 0 warnings
cargo test -p cloudmount-app      → 18 passed, 0 failed, 2 ignored (live API)
```
