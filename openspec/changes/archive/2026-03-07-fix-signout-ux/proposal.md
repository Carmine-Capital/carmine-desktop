## Why

The sign-out flow has several UX bugs that make it confusing or broken: the tray menu never updates to reflect auth state, closing the wizard after sign-out exits the app unexpectedly, the wizard reopens in a stale state ("All Set"), and the Settings Account tab always shows "Not signed in" even when authenticated. These issues make sign-out feel unreliable and can trap users into killing the app to recover.

## What Changes

- **Tray menu** now shows "Sign In…" when not authenticated and "Sign Out" when authenticated; clicking "Sign In…" opens the wizard
- **Wizard no longer exits the app when closed** while unauthenticated — the app stays alive as a tray-only process; users can re-open the wizard via "Sign In…" from the tray
- **Wizard reloads to step-welcome** after sign-out (instead of showing stale step-done content) using `WebviewWindow::reload()`
- **Settings window closes** when sign-out is triggered from the Account tab, so the wizard gets focus
- **Settings Account tab** displays the signed-in account's display name (or email if available) instead of always showing "Not signed in"

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `tray-app`: Auth-aware tray menu (Sign In / Sign Out label), wizard no longer causes app exit when closed while unauthenticated
- `app-lifecycle`: Sign-out no longer exits the process; closing the wizard while unauthenticated hides it instead of calling `exit(0)`

## Impact

- `crates/carminedesktop-app/src/tray.rs` — `update_tray_menu`, `handle_menu_event`
- `crates/carminedesktop-app/src/main.rs` — `on_window_event`
- `crates/carminedesktop-app/src/commands.rs` — `sign_out`, `get_settings`, `SettingsInfo`
- `crates/carminedesktop-app/dist/settings.html` — Account tab display, `signOut()` handler
- No new dependencies, no API surface changes, no breaking changes
