## 1. Dependency: Add tauri-plugin-dialog

- [x] 1.1 Add `tauri-plugin-dialog = "2"` to `[workspace.dependencies]` in the root `Cargo.toml`
- [x] 1.2 Add `tauri-plugin-dialog` to the `desktop` feature list and optional deps in `crates/carminedesktop-app/Cargo.toml`
- [x] 1.3 Register `tauri_plugin_dialog::init()` in `crates/carminedesktop-app/src/main.rs` alongside the other plugin registrations
- [x] 1.4 Verify `cargo build -p carminedesktop-app --features desktop` compiles cleanly with the new plugin

## 2. settings.html — Toast notification infrastructure

- [x] 2.1 Add `<div id="status-bar">` element to the HTML body (fixed-position at the bottom of the settings window)
- [x] 2.2 Add CSS for `#status-bar`: hidden by default, slide-in/fade-in transition, distinct colours for `success` and `error` variants
- [x] 2.3 Implement `showStatus(message, type)` JS helper: sets text + class on `#status-bar`, triggers transition, auto-hides after 3 seconds for success; for error, does not auto-hide (clears on next action)

## 3. settings.html — saveGeneral

- [x] 3.1 Capture the "Save" button reference at function entry; disable it and set label to "Saving…"
- [x] 3.2 On success: re-enable button, restore label "Save", call `showStatus('Settings saved', 'success')`
- [x] 3.3 On failure: re-enable button, restore label "Save", call `showStatus(e, 'error')` instead of `console.error(e)`

## 4. settings.html — saveAdvanced

- [x] 4.1 Capture the "Save" button reference at function entry; disable it and set label to "Saving…"
- [x] 4.2 On success: re-enable button, restore label "Save", call `showStatus('Settings saved', 'success')`
- [x] 4.3 On failure: re-enable button, restore label "Save", call `showStatus(e, 'error')` instead of `console.error(e)`

## 5. settings.html — toggleMount

- [x] 5.1 Locate the triggering button via `document.getElementById('toggle-btn-' + id)` (the ID is assigned during list rendering per `fix-settings-xss` task 1.1); disable it and set label to "Updating…"
- [x] 5.2 On success: call `loadMounts()` to refresh list (button is replaced by re-render); call `showStatus('Mount updated', 'success')`
- [x] 5.3 On failure: re-enable button, restore original label, call `showStatus(e, 'error')` instead of `console.error(e)`

## 6. settings.html — removeMount

- [x] 6.1 Show `confirm('Remove this mount? This cannot be undone.')` before any backend call; return early if user cancels
- [x] 6.2 Locate the triggering "Remove" button via `document.getElementById('remove-btn-' + id)` (the ID is assigned during list rendering per `fix-settings-xss` task 1.1); disable it and set label to "Removing…"
- [x] 6.3 On success: call `loadMounts()` to refresh list; call `showStatus('Mount removed', 'success')`
- [x] 6.4 On failure: re-enable button, restore label "Remove", call `showStatus(e, 'error')` instead of `console.error(e)`

## 7. settings.html — signOut

- [x] 7.1 Show `confirm('Sign out? All mounts will stop.')` before any backend call; return early if user cancels
- [x] 7.2 Capture the "Sign Out" button reference; disable it and set label to "Signing out…"
- [x] 7.3 On success: show `showStatus('Signed out', 'success')` briefly; do NOT call any window hide/close API from JS — the Rust `sign_out` command reloads the settings window directly (per `fix-window-state`), so the page will be torn down by the backend
- [x] 7.4 On failure: re-enable button, restore label "Sign Out", call `showStatus(e, 'error')` instead of `console.error(e)`

## 8. settings.html — clearCache

- [x] 8.1 Capture the "Clear Cache" button reference; disable it and set label to "Clearing…"
- [x] 8.2 Replace `alert('Cache cleared successfully.')` on success with `showStatus('Cache cleared', 'success')`; re-enable button and restore label
- [x] 8.3 Replace `alert('Failed to clear cache: ' + e)` on failure with `showStatus('Failed to clear cache: ' + e, 'error')`; re-enable button and restore label

## 9. tray.rs — sign_out confirmation

- [x] 9.1 In `handle_menu_event`, in the `"sign_out"` branch, use `tauri_plugin_dialog::DialogExt::dialog(&app)` to present an async confirmation dialog ("Sign out? All mounts will stop.") before spawning the `sign_out` task
- [x] 9.2 Only call `crate::commands::sign_out(app).await` if the user confirms; if the user cancels, log at `tracing::debug!` level and return without action

## 10. Verification

- [ ] 10.1 Manual test: save General settings — verify "Settings saved" toast appears and Save button re-enables
- [ ] 10.2 Manual test: save Advanced settings — same check as 10.1
- [ ] 10.3 Manual test: toggle a mount — verify "Mount updated" toast and refreshed list
- [ ] 10.4 Manual test: remove a mount — verify confirm dialog appears; cancel leaves mount intact; confirm removes it with success toast
- [ ] 10.5 Manual test: sign out from Account tab — verify confirm dialog; cancel aborts; confirm signs out and closes settings window
- [ ] 10.6 Manual test: clear cache — verify success toast replaces old alert()
- [ ] 10.7 Manual test: tray "Sign Out" — verify native OS dialog; cancel does nothing; confirm signs out
- [ ] 10.8 Manual test: trigger a backend error (e.g., invoke with invalid data) — verify error toast appears in status bar
- [x] 10.9 Run `cargo clippy --all-targets --all-features` — zero warnings
- [x] 10.10 Run `cargo fmt --all -- --check` — no formatting issues
