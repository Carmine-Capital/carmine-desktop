## 1. Backend — SettingsInfo account display

- [x] 1.1 Add `account_display: Option<String>` field to `SettingsInfo` in `commands.rs`
- [x] 1.2 Populate `account_display` in `get_settings`: read `effective_config.accounts.first()`, prefer `email`, fall back to `display_name`

## 2. Backend — sign_out command fixes

- [x] 2.1 In `sign_out`: hide the settings window if open (`app.get_webview_window("settings").map(|w| w.hide())`) before opening wizard
- [x] 2.2 In `sign_out`: if the wizard window exists, call `win.reload()` then `win.show()` + `win.set_focus()`; otherwise fall through to `open_or_focus_window` as before

## 3. Tray — auth-aware menu

- [x] 3.1 In `update_tray_menu`: replace the unconditional `sign_out` item with a conditional — emit `sign_in` ("Sign In…") when `!authenticated`, emit `sign_out` ("Sign Out") when `authenticated`
- [x] 3.2 In `handle_menu_event`: add `"sign_in"` arm that calls `open_or_focus_window(app, "wizard", "Setup", "wizard.html")`

## 4. App lifecycle — remove exit(0) on wizard close

- [x] 4.1 In `on_window_event` (`main.rs`): remove the `wizard + !authenticated → exit(0)` branch; let all `CloseRequested` events fall through to `window.hide()` + `api.prevent_close()`

## 5. Frontend — settings.html Account tab

- [x] 5.1 In `loadSettings()`: set `#account-email` text content from `settings.account_display` — show the display name if present, "Not signed in" if `null`
- [x] 5.2 In `signOut()`: after `await invoke('sign_out')` resolves, call `window.__TAURI__.window.getCurrentWindow().hide()` so the settings window closes and the wizard gets focus
