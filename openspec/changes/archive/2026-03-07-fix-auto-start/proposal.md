## Why

The "Start on login" toggle in Settings saves a boolean to `config.toml` but never instructs the OS to actually launch CloudMount at login. The feature has been silently non-functional on all three platforms since it was wired up, meaning users who depend on their drives being mounted after a reboot get nothing. For a filesystem daemon, surviving reboots is table-stakes reliability.

## What Changes

- **`save_settings` in `commands.rs`**: After persisting the `auto_start` flag, call `cloudmount_core::config::autostart::set_enabled()` with the current executable path to register or deregister the OS-level auto-start entry.
- **`setup_after_launch` in `main.rs`**: On startup, read `effective_config.auto_start` and call `autostart::set_enabled()` to reconcile the OS state with the persisted config value (guards against the config and OS state drifting out of sync).
- **`notify.rs`**: Add an `auto_start_failed` notification so registration errors surface to the user as a toast rather than silently vanishing into logs.
- **No new dependency**: `cloudmount_core::config::autostart` already implements all three platform backends (systemd unit on Linux, LaunchAgent plist on macOS, registry key on Windows). The module just needs to be called.

## Capabilities

### New Capabilities

None. Auto-start is already a defined capability in the tray-app and config-persistence specs.

### Modified Capabilities

- `config-persistence`: Add requirement that saving `auto_start = true/false` MUST apply the OS-level registration immediately, not just persist to TOML. Add requirement for startup sync (reconcile OS state with config on every launch).
- `tray-app`: The existing "auto-start on login (toggle)" scenario must be strengthened to require that toggling the setting actually registers/deregisters with the OS, and that failures are reported to the user via notification.

## Impact

- `crates/cloudmount-app/src/commands.rs`: `save_settings` — add `autostart::set_enabled()` call after config save; add error notification on failure (non-fatal, logged as warn).
- `crates/cloudmount-app/src/main.rs`: `setup_after_launch` — add startup sync call to `autostart::set_enabled()` with the current exe path; errors are logged but do not abort startup.
- `crates/cloudmount-app/src/notify.rs`: Add `auto_start_failed(app, reason)` helper following existing `send()` pattern.
- `crates/cloudmount-core/src/config.rs`: `autostart` module is already implemented and correct — no changes needed there.
- No new crate dependencies required.
- Affects all three platforms (Linux, macOS, Windows) via existing `#[cfg]` gates in `autostart` module.
