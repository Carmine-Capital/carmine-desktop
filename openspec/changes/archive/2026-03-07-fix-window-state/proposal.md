## Why

Settings and wizard windows display stale data because they are never reloaded when the application state changes: after sign-out, sign-in, or mount operations, the windows retain whatever DOM state was current when they were last visible. Users see the wrong account name, old mount lists, and unsaved form values that appear to have been saved.

## What Changes

- `open_or_focus_window` in `tray.rs`: when re-showing an already-created window whose label is `"settings"`, call `win.eval("loadSettings(); loadMounts();")` immediately before `win.show()` so the data is refreshed before the window becomes visible. The eval is gated on `label == "settings"` — other windows (e.g., the wizard) do not have these functions and must not receive this eval.
- No changes to `settings.html` — the refresh is driven entirely from Rust via `win.eval()`, not from a JS event listener.
- `commands.rs` (`sign_out`): call `reload()` on the settings window instead of `hide()`, so the window starts from a clean DOM on next open after sign-out.
- `wizard.html` (`cancelSignIn`): after reverting to `step-welcome`, clear the auth URL input value and hide the auth error div, so stale values from the previous attempt do not persist in the DOM.
- `tray.rs` (`open_or_focus_window`): add `min_inner_size(640.0, 480.0)` to the `WebviewWindowBuilder` call so newly created windows have a sensible minimum size.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `tray-app`: window management requirements change — settings window must refresh its data on every show, sign-out must reload (not merely hide) the settings window, wizard cancel must reset step-signing-in DOM state, and newly created windows must enforce a minimum inner size.

## Impact

- `crates/carminedesktop-app/src/tray.rs` — `open_or_focus_window` function
- `crates/carminedesktop-app/src/commands.rs` — `sign_out` function (line 169)
- `crates/carminedesktop-app/dist/wizard.html` — `cancelSignIn` function
- `openspec/specs/tray-app/spec.md` — window refresh and sign-out reload requirements
