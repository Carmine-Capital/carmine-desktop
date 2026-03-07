## Why

Platform-specific startup failures silently kill the app or produce invisible errors, leaving users with no actionable explanation. First-run experience is critical — a user who cannot see what went wrong will uninstall rather than debug.

## What Changes

- **Windows silent crash on missing client ID**: `windows_subsystem = "windows"` detaches the console in release builds, making `eprintln!` write to nonexistent stderr. Replace the `eprintln!` + `exit(1)` path with a native `MessageBoxW` dialog so users see an actionable error on double-click.
- **FUSE/macFUSE missing — only warns in logs**: `preflight_checks()` downgrades FUSE absence to a `tracing::warn!` and returns `Ok(())`. The wizard shows "All Set" while mounts silently fail. Surface FUSE absence as a post-sign-in system notification with per-platform install instructions.
- **No CfApi version check on Windows**: `preflight_checks()` has no Windows-specific guard. Cloud Files API requires Windows 10 1709+. Add a Windows version check that surfaces a dialog on unsupported versions before any mount attempt.
- **Stale FUSE mount error not shown to user**: `start_mount` logs the stale-mount error (including the `fusermount -u` remediation hint) but never surfaces it to the user. Send a system notification when `start_mount` fails so the remediation path is visible.

## Capabilities

### New Capabilities

- `platform-preflight`: Platform-specific startup validation — Windows MessageBox on fatal errors, CfApi version guard, FUSE availability notification post-sign-in, mount failure notification with remediation steps.

### Modified Capabilities

- `app-lifecycle`: Pre-flight validation failure scenario and mount lifecycle failure scenario need updated behavior — fatal errors on Windows must use a dialog instead of stderr; FUSE absence must surface a notification rather than a silent warn log; stale mount failures must send a user-visible notification.

## Impact

- `crates/cloudmount-app/src/main.rs`: `preflight_checks()` (lines 143–182), preflight call site (lines 275–278), `start_all_mounts` (lines 512–533), `start_mount` (lines 535–580).
- `crates/cloudmount-app/src/notify.rs`: New notification helpers for FUSE unavailable and mount failure.
- `Cargo.toml` (workspace): `windows` crate dependency for `MessageBoxW` on Windows desktop builds only.
- No public API changes; no breaking changes to existing config, auth, or cache behavior.
