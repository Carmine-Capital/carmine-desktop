## Context

The authentication flow uses `open::that()` (which calls `xdg-open` on Linux) to open the Microsoft login page in the default browser. This works in headless mode but silently fails in desktop mode when running as a Tauri/WebKitGTK app on Wayland with AppImage packaging. The `xdg-open` child process spawns (so `open::that()` returns `Ok(())`), but it cannot reach `xdg-desktop-portal` through D-Bus in the Tauri process context, so the browser never opens.

The `open::that()` call is hardcoded in `carminedesktop-auth::oauth::run_pkce_flow()`. The auth crate has no Tauri dependency and should not gain one — it's also used in headless mode.

## Goals / Non-Goals

**Goals:**
- Fix browser opening for the sign-in flow in desktop mode on Wayland + AppImage
- Keep `carminedesktop-auth` free of any Tauri dependency
- Preserve existing headless mode behavior

**Non-Goals:**
- Changing the OAuth2/PKCE flow itself
- Adding an in-app webview for authentication (system browser remains the approach)
- Supporting custom browser selection by the user

## Decisions

### D1: Inject opener as a closure on AuthManager

**Decision:** Add an `opener: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>` field to `AuthManager`. Pass it through `authorize()` into `run_pkce_flow()`.

**Alternatives considered:**
- *Pass opener only to `sign_in()`*: Simpler but less clean — every call site needs to provide it, and `AuthManager` already encapsulates config (client_id, tenant_id).
- *Trait-based approach (`trait UrlOpener`)*: More flexible but over-engineered for a single function. A closure is sufficient.
- *Feature-flag `open::that()` vs Tauri inside auth crate*: Violates the dependency boundary — auth crate should not know about Tauri.

**Rationale:** Storing the opener on `AuthManager` at construction time means the caller decides once how URLs should be opened, and the auth crate remains platform-agnostic.

### D2: Use `tauri-plugin-opener` for desktop mode

**Decision:** In desktop mode, construct `AuthManager` with a closure that calls `app.opener().open_url(url, None::<&str>)` via `tauri_plugin_opener::OpenerExt`. Register the plugin in the Tauri builder and add `opener:allow-open-url` to capabilities.

**Alternatives considered:**
- *Use `tauri-plugin-shell` for `shell.open()`*: Works but `tauri-plugin-opener` is the dedicated, recommended replacement in Tauri v2.
- *Open browser from the JS frontend instead*: Would require restructuring the sign-in flow to split PKCE server binding from browser opening across the IPC boundary.

**Rationale:** `tauri-plugin-opener` goes through `xdg-desktop-portal` on Wayland, which is the correct mechanism for sandboxed/AppImage apps. It's the Tauri-recommended approach for v2.

### D3: Headless mode uses `open::that()` with stderr fallback

**Decision:** In headless mode, construct `AuthManager` with a closure that calls `open::that()` (current behavior). The `has_display()` check and stderr fallback move into this closure rather than staying in `run_pkce_flow()`.

**Rationale:** The opener callback fully owns the "how to open a URL" decision, including display detection and fallback. This keeps `run_pkce_flow()` simple — it just calls the opener.

### D4: Keep `open` crate as a dependency of `carminedesktop-auth`

**Decision:** Keep the `open` crate in `carminedesktop-auth` but only use it as a default/convenience. The headless opener closure in `carminedesktop-app` imports it from there or uses its own.

**Alternatives considered:**
- *Remove `open` from auth crate entirely*: Would require the app crate to always provide an opener, even in tests. Less ergonomic.
- *Move `open` to app crate only*: Auth crate tests would need mock openers.

**Rationale:** Minimal churn. The `open` crate is tiny and the auth crate's tests can use a no-op opener.

## Risks / Trade-offs

- **[Risk] AppImage may not bundle `xdg-desktop-portal` client libraries** → Mitigation: `tauri-plugin-opener` depends on GTK/GLib which Tauri already bundles; portal communication uses D-Bus which doesn't require bundled libraries.
- **[Risk] `opener:allow-open-url` permission too restrictive** → Mitigation: Use the default permission set which allows `https://` and `http://` schemes, covering the Microsoft login URL.
- **[Trade-off] Closure stored on AuthManager adds one field** → Acceptable for clean separation of concerns.
