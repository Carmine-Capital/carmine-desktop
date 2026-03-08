---
id: fix-sign-out-clears-mounts
title: Fix sign-out to clear mount configuration
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T13:34:49.515Z
---

# Intent: Fix sign-out to clear mount configuration

## Goal

Clear `user_config.mounts` on sign-out so that reconnect — whether same account or a different one — always starts from a clean, known-good state.

## Users

All CloudMount end users who sign out and reconnect.

## Problem

`sign_out` clears `accounts` but not `mounts`. This causes two failure modes:

1. **Same-account reconnect**: `start_all_mounts()` restores mounts (correct), but the wizard shows `step-sources` with `addMountMode = false`. If the user clicks "Get Started" with OneDrive checked → `add_mount` is called for an already-mounted drive → duplicate attempt or confusing error.

2. **Account switch (A→B)**: User A's drive IDs remain in config. On sign-in as B, `start_all_mounts()` tries A's mounts with B's token → Graph 404/403. The wizard also fails to detect "already mounted" (drive ID mismatch) → B adds a new OneDrive → config ends up with A's stale mounts + B's new mount.

## Success Criteria

- After sign-out, `mounts = []` in saved config
- Reconnect with any account starts with no pre-existing mounts attempted
- Wizard flow is correct for reconnect (no duplicate add, no cross-account stale mounts)
- No 404/403 errors from stale drive IDs on reconnect
- CI passes (clippy + fmt)

## Constraints

- Change confined to `commands.rs:sign_out`
- One-liner: add `user_config.mounts.clear()` before saving config
- Does not address account-scoped persistence (that is a separate intent)

## Notes

This is the immediate pragmatic fix. The "ultimate solution" (account-scoped mounts + mount validation) is tracked in separate intents: `feat-account-scoped-mounts` and `feat-mount-validation`.
