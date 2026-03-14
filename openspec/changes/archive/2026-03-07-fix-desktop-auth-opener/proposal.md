## Why

In desktop mode (Tauri/WebKitGTK) on Wayland with AppImage packaging, clicking "Sign in with Microsoft" logs "opening browser for authentication" but the browser never opens. The `open::that()` crate calls `xdg-open` which spawns successfully (returns `Ok(())`) but silently fails because the Tauri/WebKitGTK process context disrupts D-Bus/portal access needed on Wayland. Headless mode works fine because Tauri/WebKitGTK never initializes.

## What Changes

- Inject a URL opener callback into `AuthManager` instead of hardcoding `open::that()` in `run_pkce_flow`
- Desktop mode passes a Tauri opener (`tauri-plugin-opener`) that uses `xdg-desktop-portal` correctly
- Headless mode passes `open::that()` (preserving current working behavior)
- Add `tauri-plugin-opener` dependency and Tauri capability permissions

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `microsoft-auth`: The PKCE browser-opening mechanism becomes injectable rather than hardcoded, allowing the caller to provide a platform-appropriate URL opener.

## Impact

- `carminedesktop-auth`: `AuthManager` gains an `opener` field; `run_pkce_flow` accepts an opener parameter instead of calling `open::that()` directly. The `open` crate dependency may become optional or be removed.
- `carminedesktop-app`: Desktop mode constructs `AuthManager` with a Tauri-based opener; headless mode uses `open::that()`.
- New dependency: `tauri-plugin-opener` (workspace, behind `desktop` feature flag).
- Tauri capabilities config needs `opener:allow-open-url` permission.
