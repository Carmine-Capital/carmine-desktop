## 1. Notification Helper

- [x] 1.1 Add `auto_start_failed(app: &AppHandle, reason: &str)` function to `crates/cloudmount-app/src/notify.rs`, following the existing `send()` pattern with title "Auto-start" and body `format!("Failed to register auto-start: {reason}")`

## 2. Wire Auto-Start in save_settings

- [x] 2.1 In `crates/cloudmount-app/src/commands.rs`, import `cloudmount_core::config::autostart` at the top of the `#[cfg(feature = "desktop")]` section
- [x] 2.2 After the existing `user_config.save_to_file(...)` and `rebuild_effective_config(...)` calls in `save_settings`, resolve the current executable path via `std::env::current_exe()`
- [x] 2.3 When `auto_start` is `Some(v)`, call `autostart::set_enabled(v, &exe_path)` using the resolved path; on error, call `notify::auto_start_failed(&app, &e.to_string())` and log `tracing::warn!`; do not return an error from the command
- [x] 2.4 Verify that the success path logs `tracing::info!("auto-start {}", if v { "enabled" } else { "disabled" })` for observability

## 3. Startup Reconciliation in setup_after_launch

- [x] 3.1 In `crates/cloudmount-app/src/main.rs`, in `setup_after_launch`, after the `state.authenticated.store(true, ...)` call (i.e., after successful token restore), resolve `effective_config.auto_start` from the locked config
- [x] 3.2 Call `std::env::current_exe()` to get the exe path; on failure, log `tracing::warn!` and skip the sync
- [x] 3.3 Call `cloudmount_core::config::autostart::set_enabled(auto_start, &exe_path)`; on error, log `tracing::warn!` only (no notification on startup — the user is not actively interacting with the UI at this point)
- [x] 3.4 Ensure the reconciliation call is placed before `start_all_mounts(app)` so auto-start state is settled before mounts begin

## 4. Verification

- [ ] 4.1 Manually test on Linux: enable auto-start in Settings, verify `~/.config/systemd/user/cloudmount.service` is created and `systemctl --user is-enabled cloudmount.service` reports `enabled`; disable and verify the file is removed and the service is disabled
- [ ] 4.2 Manually test on macOS: enable auto-start, verify `~/Library/LaunchAgents/com.cloudmount.agent.plist` exists with correct `ProgramArguments`; disable and verify plist is removed
- [ ] 4.3 Manually test on Windows: enable auto-start, verify `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CloudMount` registry value exists with the correct exe path; disable and verify the value is removed
- [ ] 4.4 Simulate `current_exe()` failure (or exe path pointing to a non-existent binary) and confirm the `auto_start_failed` notification appears on toggle, and warn log appears on startup without crashing
- [x] 4.5 Run `cargo clippy --all-targets --all-features` and confirm zero warnings
- [x] 4.6 Run `cargo fmt --all -- --check` and confirm no formatting issues
- [x] 4.7 Run `cargo test --all-targets` and confirm no regressions
