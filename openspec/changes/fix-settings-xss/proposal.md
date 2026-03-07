## Why

The settings UI (`settings.html`) builds mount list entries via `innerHTML` string concatenation with data from Tauri IPC, which originates from user config on disk. This is a DOM-based XSS vulnerability: a malicious or corrupted `~/.config/cloudmount/config.toml` can embed HTML/JavaScript in mount names, mount paths, or mount IDs, and that payload executes inside the Tauri webview the moment `loadMounts()` runs. The Tauri webview shares the IPC bridge with all privileged backend commands, so code executing there can invoke any command the frontend is authorized to call. There is also no Content-Security-Policy declared in either `settings.html` or `wizard.html` to constrain what scripts can run.

## What Changes

- Replace `innerHTML` string concatenation in `loadMounts()` with safe DOM API calls (`createElement`, `textContent`, `.onclick` function reference) so no user-controlled string is ever parsed as HTML.
- Bind `onclick` handlers for toggle/remove buttons as JavaScript function references rather than injecting mount IDs into HTML attribute strings.
- Add a `<meta http-equiv="Content-Security-Policy">` tag to `settings.html` and `wizard.html` that prohibits inline event handlers injected through attribute string construction and restricts script sources to `'self'`.

## Capabilities

### New Capabilities

- `settings-xss-hardening`: Safe DOM rendering of mount list entries and Content-Security-Policy enforcement for the settings and wizard webview pages.

### Modified Capabilities

- `tray-app`: The Settings window requirement gains an explicit security constraint — mount list entries in the UI MUST be rendered via safe DOM APIs; no user-supplied string MAY be interpolated into HTML markup or inline event handler attributes.

## Impact

- `crates/cloudmount-app/dist/settings.html` — `loadMounts()` function and `<head>` CSP meta tag (primary fix).
- `crates/cloudmount-app/dist/wizard.html` — `<head>` CSP meta tag (defensive hardening; no innerHTML data injection present but CSP is missing).
- `openspec/specs/tray-app/spec.md` — delta spec adds security requirement to the Settings window section.
- No backend Rust changes. No new dependencies. No API or IPC changes.
