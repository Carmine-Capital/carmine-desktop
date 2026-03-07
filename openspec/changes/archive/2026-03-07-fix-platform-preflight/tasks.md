## 1. Windows dependency setup

- [x] 1.1 Add `windows` crate to workspace `[workspace.dependencies]` with feature flags `Win32_UI_WindowsAndMessaging` and `Win32_System_SystemInformation`, gated to Windows targets only
- [x] 1.2 Add `windows = { workspace = true }` to `crates/cloudmount-app/Cargo.toml` under `[target.'cfg(windows)'.dependencies]`

## 2. Windows fatal error dialog helper

- [x] 2.1 Implement `show_error_dialog(title: &str, msg: &str)` in `crates/cloudmount-app/src/main.rs` behind `#[cfg(all(target_os = "windows", feature = "desktop", not(debug_assertions)))]` using `MessageBoxW`
- [x] 2.2 Implement a `fatal_error(msg: &str) -> !` helper that calls `show_error_dialog` on Windows release desktop builds and `eprintln!` + `exit(1)` on all other configurations
- [x] 2.3 Replace the `eprintln!("Error: {msg}"); std::process::exit(1);` call site (main.rs ~276–278) with a call to `fatal_error(&msg)`

## 3. Windows CfApi version check

- [x] 3.1 Add a `#[cfg(target_os = "windows")]` block to `preflight_checks()` that calls `VerifyVersionInfoW` to verify Windows 10 build >= 16299 (version 10.0.16299)
- [x] 3.2 On version check failure, return `Err(...)` with the message "Cloud Files API requires Windows 10 version 1709 (build 16299) or later"
- [x] 3.3 Verify that `fatal_error` (task 2.3) correctly displays the CfApi version error via `MessageBoxW` on Windows release desktop builds

## 4. FUSE unavailable notification

- [x] 4.1 Add `pub fn fuse_unavailable(app: &AppHandle)` to `crates/cloudmount-app/src/notify.rs` behind `#[cfg(any(target_os = "linux", target_os = "macos"))]`, with Linux body "Filesystem mounts require FUSE. Run: sudo apt install fuse3 (Debian/Ubuntu) or equivalent for your distribution." and macOS body "Filesystem mounts require macFUSE. Install it from https://github.com/osxfuse/osxfuse/releases."
- [x] 4.2 Add the FUSE availability check in **both** of the following locations (each covers a distinct code path that leads to `start_all_mounts`):
  - In `setup_after_launch` (main.rs), immediately before the `start_all_mounts` call — this covers returning users whose tokens are restored from the keyring without going through the sign-in wizard
  - In `complete_sign_in` (commands.rs), after the sign-in command succeeds and before mounts are started — this covers first-time or re-authenticating users
  - In both locations, add a `#[cfg(any(target_os = "linux", target_os = "macos"))]` block that probes FUSE availability (same check as `preflight_checks`) and calls `notify::fuse_unavailable(app)` if FUSE is absent
- [x] 4.3 Confirm the `tracing::warn!` in `preflight_checks` for FUSE absence is kept (log + notification both fire; they serve different audiences)

## 5. Mount failure notification

- [x] 5.1 Add `pub fn mount_failed(app: &AppHandle, name: &str, reason: &str)` to `crates/cloudmount-app/src/notify.rs` that sends a notification titled "Mount Failed" with body `"{name}: {reason}"`
- [x] 5.2 In `start_all_mounts` (main.rs ~527–532), when `start_mount` returns `Err(e)`, call `notify::mount_failed(app, &mount_config.name, &e)` alongside the existing `tracing::error!` call

## 6. Tests

- [x] 6.1 Add a unit test in `crates/cloudmount-app/tests/` (or inline) verifying that `preflight_checks` returns `Err` containing "Windows 10 version 1709" when the mock version check fails (Windows only, `#[cfg(target_os = "windows")]`)
- [x] 6.2 Add a unit test verifying that `preflight_checks` returns `Err` containing the placeholder client ID message when the default client ID is passed (cross-platform)
- [x] 6.3 Verify that `cargo clippy --all-targets --all-features` passes with zero warnings after all changes

## 7. Verification

- [ ] 7.1 On Linux: confirm that launching with the placeholder client ID shows a visible terminal error (not silent)
- [ ] 7.2 On Linux without FUSE: confirm that a system notification appears after sign-in with the `sudo apt install fuse3` instruction
- [ ] 7.3 On Linux with a stale FUSE mount: confirm a "Mount Failed" notification appears with the `fusermount -u` remediation hint
- [ ] 7.4 On Windows release build: confirm that launching with the placeholder client ID shows a `MessageBoxW` dialog instead of silently exiting
- [ ] 7.5 On Windows release build: confirm that the CfApi version check dialog is shown on a simulated unsupported version (or by temporarily lowering the version threshold in a test build)
