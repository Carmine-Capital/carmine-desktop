## Context

The sign-out flow has multiple interacting bugs rooted in how the wizard window is managed:

1. The wizard window is never destroyed — `on_window_event` intercepts all `CloseRequested` events and hides windows via `window.hide()` + `api.prevent_close()`. The only exception was `wizard + !authenticated → exit(0)`.
2. `open_or_focus_window` finds the hidden wizard and shows it without reloading the page — stale JS state (e.g. `step-done`) is displayed.
3. `update_tray_menu` always emits `sign_out` regardless of auth state.
4. `settings.html` never populates the account email element (`#account-email`), and `SettingsInfo` carries no account data.
5. Sign-out from the settings window leaves the settings window open, so the wizard opens behind it and the user never sees it.

Key constraint: Tauri 2.10.3. `WebviewWindow` has `reload()` (→ `WebviewMessage::Reload` → WRY native reload), confirmed available.

## Goals / Non-Goals

**Goals:**
- Wizard always shows `step-welcome` when opened post-sign-out
- Tray menu label reflects auth state ("Sign In…" / "Sign Out")
- Closing the wizard never exits the process; app survives as tray-only
- Settings Account tab shows the signed-in display name
- Settings window closes when sign-out is triggered from within it

**Non-Goals:**
- Full account management UI (email editing, multi-account)
- Wizard UX redesign or step logic changes
- Headless mode changes

## Decisions

### D1 — Use `win.reload()` to reset wizard state on sign-out

When `commands::sign_out` needs to show the wizard, if the window already exists it calls `win.reload()` before `win.show()`. This causes the webview to do a native WRY reload, running `init()` fresh at `step-welcome`.

**Alternatives considered:**
- `win.eval("location.reload()")` — JavaScript-based, slightly less reliable on hidden windows
- `win.navigate(url)` — requires a `Url` (not `WebviewUrl`), more verbose with no benefit
- Destroy and recreate the window — triggers `CloseRequested` which would hit the `exit(0)` path (timing hazard since `authenticated` is already `false`); also flickers

`reload()` is the safest and most idiomatic choice.

### D2 — Remove `exit(0)` from `on_window_event`

The `wizard + !authenticated → exit(0)` branch was added to handle first-run abandonment. With the new design:
- Closing the wizard always hides it (same as every other window)
- The tray always has "Sign In…" when unauthenticated, giving the user a path back
- Users can quit via "Quit carminedesktop" from the tray at any time

This is a behavior change: previously, closing the wizard on first run exited the process. Now the app stays alive. This is acceptable because the tray is always present and functional.

### D3 — Auth-aware tray menu: "Sign In…" / "Sign Out"

`update_tray_menu` reads `state.authenticated` (already available). When `!authenticated`, it emits a `sign_in` menu item instead of `sign_out`. `handle_menu_event("sign_in")` calls `open_or_focus_window` for the wizard.

The initial menu built in `tray::setup` (before `setup_after_launch` completes) still shows `sign_out` for a brief moment — acceptable since `update_tray_menu` is called moments later.

### D4 — Add `account_display` to `SettingsInfo`

`get_settings` already reads `effective_config`. Adding `account_display: Option<String>` populated from `effective_config.accounts.first()` (preferring `email`, falling back to `display_name`) costs nothing. `settings.html` then sets `#account-email` in `loadSettings()`.

**Alternative:** separate `get_account` command — unnecessary overhead for a single optional field.

### D5 — Hide settings window in `sign_out` when triggered from settings

`commands::sign_out` calls `app.get_webview_window("settings").map(|w| w.hide())` before opening the wizard. This ensures the wizard gets focus regardless of trigger path (tray or settings UI).

## Risks / Trade-offs

- **Brief wizard white-flash on reload**: `win.reload()` initiates an async reload; `win.show()` follows immediately. The window may flash white for a few milliseconds during page load. Inherent to web-based UIs; identical to creating a new window. Acceptable.
- **First-run app-stays-alive on wizard close**: App now lives in tray if user closes wizard without signing in on first launch. This is the intended new behavior per the user's request, but represents a design change from the original "exit on first-run wizard close" intent.
- **account_display is drive name, not email**: `complete_sign_in` stores `display_name = drive.name` (e.g. "OneDrive") and `email = None`. The Account tab will show the drive name, not the user's email. This is a known limitation of the current Graph query (only `GET /me/drive` is called); fetching `/me` for the user profile is future work.

## Open Questions

None — all decisions are locked.
