## Context

carminedesktop's settings UI exposes a "Start on login" toggle. When the user saves the setting, `save_settings` in `commands.rs` writes `auto_start = true/false` to `~/.config/carminedesktop/config.toml` and rebuilds `EffectiveConfig` — but stops there. The OS-level registration is never performed. As a result, the toggle has been a no-op since the feature was first wired up.

The platform implementation already exists and is complete: `carminedesktop_core::config::autostart` contains `set_enabled(enabled, app_path)` with correct, tested backends for all three target OSes:

- **Linux**: Writes `~/.config/systemd/user/carminedesktop.service` then runs `systemctl --user enable carminedesktop.service`. Disable removes the file and runs `systemctl --user disable`.
- **macOS**: Writes a LaunchAgent plist to `~/Library/LaunchAgents/com.carminedesktop.agent.plist`. Disable removes the file (the `launchd` daemon picks up the change automatically on next login).
- **Windows**: Calls `reg add HKCU\...\Run /v carminedesktop /d <path>`. Disable calls `reg delete`.

The only gap is that `save_settings` and `setup_after_launch` never call this module.

## Goals / Non-Goals

**Goals:**

- Call `autostart::set_enabled()` from `save_settings` whenever the `auto_start` value changes, so toggling the setting in the UI actually registers/deregisters with the OS.
- Call `autostart::set_enabled()` during `setup_after_launch` to reconcile OS state with the persisted config value (handles cases where the OS entry was manually removed, or the exe path changed after an update).
- Surface registration failures to the user via a toast notification rather than silently swallowing the error.
- Log all auto-start outcomes (`tracing::info!` on success, `tracing::warn!` on failure).

**Non-Goals:**

- Replacing or rewriting the `autostart` module in `carminedesktop-core` — it is correct as-is.
- Supporting `tauri-plugin-autostart` — the native module already covers all three platforms without an additional dependency.
- Auto-start for headless mode — headless invocations are out of scope; the auto-start feature is desktop-only (gated behind `#[cfg(feature = "desktop")]`).
- Handling systemd socket activation or launchd on-demand launch — a simple `ExecStart` / `RunAtLoad` is sufficient.

## Decisions

### D1: Use existing `carminedesktop_core::config::autostart` rather than adding `tauri-plugin-autostart`

**Decision**: Call the already-implemented `autostart::set_enabled()` from `commands.rs` and `main.rs` rather than adding `tauri-plugin-autostart` as a new workspace dependency.

**Rationale**: The native module is fully implemented, covers all three platforms with the correct mechanism for each, and has zero additional dependencies. Adding `tauri-plugin-autostart` would introduce a crate dependency to solve a problem that is already solved. The Tauri plugin wraps the same OS primitives (systemd, LaunchAgent, registry) with no meaningful advantage for this use case.

**Alternative considered**: `tauri-plugin-autostart` — rejected because it adds a dependency for functionality we already ship.

### D2: Obtain exe path via `std::env::current_exe()`

**Decision**: Use `std::env::current_exe()` to get the path to pass to `autostart::set_enabled()`.

**Rationale**: This is the canonical, cross-platform way to get the running executable's path. On Linux it reads `/proc/self/exe`; on macOS it uses `_NSGetExecutablePath`; on Windows it calls `GetModuleFileNameW`. No Tauri-specific API is needed.

**Failure handling**: If `current_exe()` fails (extremely unlikely in practice), log the error as `warn!` and send the `auto_start_failed` notification. Do not abort the operation.

### D3: Non-fatal error handling — warn + notify, do not fail the command

**Decision**: Errors from `autostart::set_enabled()` are non-fatal. `save_settings` still returns `Ok(())` after emitting a warning log and a toast notification. Startup sync errors are always non-fatal.

**Rationale**: A failed auto-start registration is a recoverable inconvenience, not a critical error. The config has already been persisted correctly. Returning an error from `save_settings` would roll back the UI state and confuse the user. The toast notification gives them actionable feedback ("Auto-start registration failed: ...") without blocking anything else.

**Alternative considered**: Return `Err(...)` from `save_settings` — rejected because it breaks the UI flow for a non-critical side effect.

### D4: Startup sync on every launch

**Decision**: During `setup_after_launch`, after tokens are restored, call `autostart::set_enabled(effective_config.auto_start, exe_path)` unconditionally (not just when the config value differs from OS state).

**Rationale**: There is no reliable way to query the current OS registration state across all three platforms without additional logic (querying systemd, reading the plist file, checking the registry). Calling `set_enabled` unconditionally is idempotent — enabling an already-enabled entry is harmless, disabling an already-disabled entry is harmless — and keeps the implementation simple.

**Placement**: The sync call goes after token restoration and before `start_all_mounts`, so the auto-start state is settled before mounts begin.

## Risks / Trade-offs

- **Systemd unavailable on Linux** → `systemctl` may not be installed in all Linux environments (e.g., some container or non-systemd distros). Mitigation: The `output()` call does not propagate the exit code; the service file is still written. The warning notification tells the user to enable it manually. This matches the existing implementation's behavior.
- **Exe path changes after update (Linux/macOS)** → The startup sync in `setup_after_launch` re-registers with the current exe path on each launch, so the service file / plist stays up to date automatically after in-place binary updates. No special handling needed.
- **Windows UAC** → The registry key is in `HKCU` (current user hive), which does not require elevation. No UAC prompt.
- **macOS Gatekeeper / notarization** → LaunchAgents for notarized apps work without quarantine issues. No special handling needed beyond what the existing plist template already does.
- **Race on first launch** → `setup_after_launch` runs asynchronously after Tauri setup. The auto-start sync happens inside this async task, which is fine — it does not block the UI thread.

## Migration Plan

This is a pure additive fix with no data migration:

1. Ship the change. On next app launch, `setup_after_launch` syncs the OS state to match the persisted `auto_start` value. Users who had `auto_start = true` in their config will have the OS entry created automatically.
2. Users who had `auto_start = false` (the default) are unaffected — `set_enabled(false, ...)` is a no-op when no entry exists.
3. No rollback needed — removing the OS entry (if any) is the disable path, which was always correct.

## Open Questions

None. The implementation is fully specified by the existing `autostart` module and the call-sites identified above.
