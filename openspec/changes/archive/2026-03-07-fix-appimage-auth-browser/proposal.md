## Why

On immutable Linux (Fedora Silverblue/Aurora), AppImage is the only viable desktop delivery format. The AppImage runtime sets `LD_LIBRARY_PATH` to its bundled GTK/GLib/WebKit2GTK libs, which causes `xdg-open`'s internal `gio open` to pick up the wrong GLib and silently fail — leaving the browser closed and the auth flow hanging. The current opener (`tauri_plugin_opener`) gives no control over child process environment, and the fire-and-forget spawn means the failure is unobservable (no error logged). Additionally, when `xdg-open` does fail, there is no in-GUI fallback — the only fallback prints to stderr, which is invisible in a GUI AppImage.

## What Changes

- **Replace `tauri_plugin_opener.open_url()`** in the desktop opener closure (Linux only) with a direct `std::process::Command` invocation of `xdg-open`, stripping `LD_LIBRARY_PATH` and `LD_PRELOAD` from the child process environment before spawning.
- **Use `.status()` instead of `.spawn()`** so the opener waits for `xdg-open` to exit and can surface a real error code — enabling the existing `oauth.rs` fallback to activate on failure.
- **Show the auth URL in the wizard UI** when sign-in is initiated, as a copy-paste fallback in case browser launch fails for any reason (AppImage env, sandboxing, headless, etc.).

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `microsoft-auth`: New requirement that the desktop auth flow provides an in-GUI URL fallback when the browser cannot be opened.
- `tray-app`: Wizard UI must display the auth URL during sign-in flow.

## Impact

- `crates/carminedesktop-app/src/main.rs` — desktop opener closure (`run_desktop`, ~line 318)
- `crates/carminedesktop-app/src/commands.rs` — `sign_in` command must surface the auth URL to the frontend
- Wizard frontend — display auth URL + copy button when sign-in is in progress
- `crates/carminedesktop-auth/src/oauth.rs` — `run_pkce_flow` must return or communicate the auth URL back to the caller so it can be forwarded to the UI
- No new dependencies
