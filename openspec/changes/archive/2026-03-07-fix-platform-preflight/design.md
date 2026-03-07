## Context

CloudMount runs on Linux, macOS, and Windows. Each platform has a distinct class of startup failures that the current code either swallows silently or emits only to the logger:

1. **Windows + `windows_subsystem = "windows"`**: The `#![cfg_attr(all(not(debug_assertions), feature = "desktop"), windows_subsystem = "windows")]` attribute detaches the process from its console in release builds. Any `eprintln!` call at the `preflight_checks` failure site writes to a handle that does not exist. The process exits with code 1 and the window vanishes with no trace visible to the user.

2. **Linux/macOS FUSE**: `preflight_checks` probes `fusermount3` / `fusermount` and, on failure, emits a `tracing::warn!` and returns `Ok(())`. The wizard flow completes normally, sign-in succeeds, and then every mount silently fails. The tray shows "Error" with no explanation.

3. **Windows CfApi version**: There is no Windows-specific check in `preflight_checks`. Cloud Files API requires Windows 10 build 1709 (Fall Creators Update, 16299+). Calling into CfApi on an older build produces a cryptic COM error with no user-facing context.

4. **Stale FUSE mount at startup**: `start_mount` returns `Err(String)` when `cleanup_stale_mount` fails. `start_all_mounts` logs the error via `tracing::error!` and silently continues. The mount appears as "Unmounted" in the tray with no explanation or remediation path shown to the user.

All four issues share the same root cause: errors that require user action are being routed to the log sink instead of to the user.

## Goals / Non-Goals

**Goals:**

- Replace `eprintln!` + `exit(1)` on Windows with a blocking `MessageBoxW` dialog before the process terminates, so the error message is visible even in GUI-only release builds.
- Add a Windows version check in `preflight_checks` that shows a `MessageBoxW` dialog and exits when CfApi is unavailable (Windows < 1709).
- Promote FUSE absence from a warn-log to a post-sign-in system notification with per-platform install instructions, without blocking the sign-in flow.
- Send a system notification when `start_mount` fails, including the error string (which already contains the `fusermount -u` remediation hint for stale mount errors).
- Add `notify::fuse_unavailable` and `notify::mount_failed` helpers following the existing `notify::send` pattern.

**Non-Goals:**

- Automatic FUSE installation or remediation.
- Surfacing errors in headless mode via dialog (headless already writes to a visible terminal; `eprintln!` is correct there).
- Changing the FUSE probe logic itself — only the error disposition changes.
- Modifying the error representation returned from `start_mount` or `preflight_checks`.

## Decisions

### D1: Windows fatal errors use `MessageBoxW`, not Tauri dialog

**Decision**: Use `windows::Win32::UI::WindowsAndMessaging::MessageBoxW` (from the `windows` crate) directly, behind `#[cfg(all(target_os = "windows", feature = "desktop", not(debug_assertions)))]`.

**Rationale**: Tauri's blocking dialog API (`tauri::api::dialog::blocking::message`) is not available before the Tauri app builder runs, and `preflight_checks` is called before `run_desktop`. A native Win32 call has no runtime dependencies and works in the exact context where the failure occurs. In debug builds, `debug_assertions` is true, so `windows_subsystem = "windows"` is not set and `eprintln!` still works — no change needed for debug.

**Alternative considered**: Show a Tauri window by deferring the preflight check until after `tauri::Builder` is initialized. Rejected because it inverts the initialization order (preflight is intentionally first) and adds significant complexity.

**Alternative considered**: Write to a log file and display the path. Rejected because a user who just double-clicked an `.exe` will not know where to find the log file.

### D2: FUSE absence is a post-sign-in notification, not a blocking preflight failure

**Decision**: Keep `preflight_checks` returning `Ok(())` when FUSE is absent. Instead, call a new `notify::fuse_unavailable(app)` helper from **both** `setup_after_launch` (for returning users whose tokens are restored from the keyring) and `complete_sign_in` (for users who have just signed in for the first time or after re-authentication), in both cases before calling `start_all_mounts`. The `#[cfg(any(target_os = "linux", target_os = "macos"))]` gate applies to both call sites.

**Rationale**: FUSE absence is not fatal — the user can still sign in, browse SharePoint sites, and configure mounts; the notification tells them what to install and the app remains usable. Blocking on preflight would prevent sign-in on a machine where FUSE is momentarily unavailable or being upgraded. The notification fires before mounts start because that is the earliest point where an `AppHandle` is available and the user has a meaningful context to act on the message. Both code paths that lead to `start_all_mounts` must emit the notification so returning users (token-restore path) are equally informed as first-time users (sign-in wizard path).

**Alternative considered**: Show a native dialog (like D1) before Tauri init. Rejected because FUSE absence is a soft warning, not a fatal configuration error. A dialog at launch would be alarming and block the app from opening.

**Alternative considered**: Keep the `tracing::warn!` and add a tray badge. Rejected because the tray badge would require polling FUSE state continuously; a one-shot notification is simpler and sufficient.

### D3: Stale mount failures send a notification via `notify::mount_failed`

**Decision**: In `start_all_mounts`, when `start_mount` returns `Err(e)`, call `notify::mount_failed(app, &mount_config.name, &e)` in addition to `tracing::error!`.

**Rationale**: The error string returned from `start_mount` for a stale FUSE mount already contains the remediation hint (`fusermount -u <path>`). Forwarding it directly to the notification body requires no new logic. The notification system (Tauri plugin) handles truncation on platforms with body length limits.

**Alternative considered**: Show a Tauri dialog (blocking). Rejected because multiple mounts can fail at startup; multiple blocking dialogs would be disruptive.

### D4: CfApi version check uses `windows::Win32::System::SystemInformation::VerifyVersionInfoW`

**Decision**: Add a `#[cfg(target_os = "windows")]` block to `preflight_checks` that calls `VerifyVersionInfoW` to check for Windows 10 build >= 16299. On failure, call the same `show_error_dialog` helper used for D1 and return `Err(...)`.

**Rationale**: `VerifyVersionInfoW` is the documented Win32 API for version gating. It is available in the same `windows` crate pulled in for D1. The check is fast (microseconds) and has no side effects.

**Alternative considered**: `winver` or `os_version` crates. Rejected to keep the dependency footprint minimal — the `windows` crate is already required for D1.

## Risks / Trade-offs

- **`windows` crate compile time**: Adding the `windows` crate increases Windows build time. Mitigated by selecting only the `Win32_UI_WindowsAndMessaging` and `Win32_System_SystemInformation` feature flags, keeping the footprint small.
- **Notification body truncation**: Some platforms (notably macOS) truncate notification body text. The stale mount error string includes a `fusermount -u <path>` command that may be cut off on long paths. Mitigated by keeping the error string concise; the full error is always available in the log.
- **FUSE notification fires on every sign-in**: If the user signs out and signs in again on a machine without FUSE, the notification fires again. This is acceptable — it is a reminder, not an error. The notification is only shown if FUSE is absent.

## Migration Plan

1. Add `windows` crate to workspace `[workspace.dependencies]` with minimal feature flags (Windows builds only; the crate is no-op on Linux/macOS).
2. Implement `show_error_dialog` helper in `main.rs` behind the Windows + desktop + release gate.
3. Replace the `eprintln!` + `exit(1)` call site with a platform-dispatched helper that calls `show_error_dialog` on Windows release builds and `eprintln!` elsewhere.
4. Add `#[cfg(target_os = "windows")]` block in `preflight_checks` for the CfApi version check.
5. Add `notify::fuse_unavailable` and `notify::mount_failed` in `notify.rs`.
6. Wire `notify::fuse_unavailable` into `setup_after_launch` (before `start_all_mounts`, for returning users) and into `complete_sign_in` (before starting mounts, for fresh sign-in users).
7. Wire `notify::mount_failed` into `start_all_mounts`.

No data migrations, config changes, or API changes are required. The change is purely additive in behavior — no existing behavior is removed.

## Open Questions

- None. All technical decisions are resolved above.
