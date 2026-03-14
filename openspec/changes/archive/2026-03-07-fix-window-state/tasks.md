## 1. Rust: Refresh Settings on Window Re-show

- [x] 1.1 In `crates/carminedesktop-app/src/tray.rs`, update `open_or_focus_window`: in the branch where `app.get_webview_window(label)` returns `Some(win)`, add `if label == "settings" { let _ = win.eval("loadSettings(); loadMounts();"); }` immediately before `win.show()` — the guard is mandatory: other windows (e.g., the wizard) do not define these functions and calling them would throw a silent `ReferenceError` in the webview
- [x] 1.2 In the same `open_or_focus_window` function, add `.min_inner_size(640.0, 480.0)` to the `WebviewWindowBuilder` chain in the `else` branch (new window creation) so all newly created windows enforce the minimum inner size

## 2. Rust: Reload Settings Window on Sign-Out

- [x] 2.1 In `crates/carminedesktop-app/src/commands.rs`, update the `sign_out` command: replace the `app.get_webview_window("settings").map(|w| w.hide())` call (line 169) with logic that calls `w.reload()` instead of `w.hide()`, ensuring the settings window starts from a clean DOM on the next open after sign-out

## 3. JS: Wizard Cancel Cleanup

- [x] 3.1 In `crates/carminedesktop-app/dist/wizard.html`, update the `cancelSignIn()` function: after `showStep('step-welcome')`, add `document.getElementById('auth-url').value = '';` to clear the stale auth URL from the input
- [x] 3.2 In the same `cancelSignIn()` function, add `const errEl = document.getElementById('auth-error'); errEl.style.display = 'none'; errEl.textContent = '';` to hide and clear any error message left from a prior sign-in attempt

## 4. Verification

- [ ] 4.1 Manual test STALE-1: open settings, change sync interval dropdown to a different value, close the window without saving, reopen — verify the dropdown shows the saved value (not the unsaved change)
- [ ] 4.2 Manual test STALE-2: sign in, open settings to confirm account name appears, sign out via tray menu, sign back in, open settings — verify the Account tab shows the new account name (not the pre-sign-out name) and mount list is current
- [ ] 4.3 Manual test M-004: open wizard, click "Sign in with Microsoft", wait for the auth URL to appear in the input, click "Cancel" — verify the welcome step is shown with an empty auth URL input and no error message visible
- [x] 4.4 Run `cargo clippy --all-targets --all-features` and confirm zero warnings (the `win.eval()` return value must be handled with `let _ =`)
- [x] 4.5 Run `cargo fmt --all -- --check` to confirm formatting is clean
