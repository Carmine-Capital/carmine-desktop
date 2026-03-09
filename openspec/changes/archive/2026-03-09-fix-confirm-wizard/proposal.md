## Why

Three UI interactions are silently broken: Sign Out and Remove Mount buttons do nothing because `window.confirm()` is not functional in Tauri 2 webviews (silently returns `undefined`), and the Add Mount flow (from both settings and the tray menu) opens the wizard at the sign-in screen even when the user is already authenticated.

## What Changes

- Replace all `window.confirm()` calls in `settings.js` with `window.__TAURI__.dialog.confirm()` from the Tauri dialog plugin, which works correctly in webview contexts
- Add `dialog:allow-confirm` permission to `capabilities/default.json` so the dialog plugin JS API is accessible from the webview
- Add a lightweight `is_authenticated` Tauri command that reads `AppState.authenticated` without a network call
- Make `wizard.js` check authentication state on load; if already signed in, skip directly to the add-sources step (`step-sources`) instead of the sign-in screen
- When re-focusing an existing wizard window for the "add mount" action, navigate it to the sources step via `win.eval()`

## Capabilities

### New Capabilities

*(none — all changes are fixes to existing functionality)*

### Modified Capabilities

- `tray-app`: Destructive confirmation dialogs in settings (Sign Out, Remove Mount) must use the Tauri dialog plugin rather than `window.confirm()`
- `sharepoint-browser`: The setup wizard must detect authentication state on load and route to the appropriate step (sources if already authenticated, sign-in if not)

## Impact

- `crates/cloudmount-app/dist/settings.js` — replace `window.confirm()` calls
- `crates/cloudmount-app/dist/wizard.js` — add auth check in `init()`, expose `goToAddMount()` helper
- `crates/cloudmount-app/src/commands.rs` — add `is_authenticated` command, register in invoke handler
- `crates/cloudmount-app/src/main.rs` — register new command in `invoke_handler!`
- `crates/cloudmount-app/src/tray.rs` — call `win.eval("goToAddMount()")` when focusing existing wizard for add-mount
- `crates/cloudmount-app/capabilities/default.json` — add `dialog:allow-confirm`
- No new dependencies; `tauri-plugin-dialog` is already a workspace dependency
