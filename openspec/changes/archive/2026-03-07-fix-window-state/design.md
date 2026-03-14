## Context

Tauri webview windows are created once and then shown/hidden across their lifetime. When a window is hidden and later reshown via `open_or_focus_window`, it renders exactly the DOM state it had when it was last visible. This creates three distinct stale-state bugs:

1. **Settings window data staleness** (STALE-1, X-015): `loadSettings()` and `loadMounts()` run once at page load. If the user changes a form value without saving and closes the window, the unsaved value persists on next open. Likewise, after a mount is added/removed externally, the settings window does not reflect the new list until the page is reloaded.

2. **Settings window not reloaded after sign-out** (STALE-2): `sign_out` in `commands.rs` calls `hide()` on the settings window. On next open (after signing back in), the window shows the old account display name and pre-sign-out mount list.

3. **Wizard DOM stale after cancel** (M-004): `cancelSignIn()` reverts to `step-welcome` but does not clear the auth URL input or the error message div. On the next sign-in attempt, the stale URL is briefly visible before being overwritten.

The fix is purely in the app layer (`carminedesktop-app`): no other crates are affected. The Tauri `Emitter` trait is already imported in `commands.rs`, so emitting an event requires no new dependencies.

## Goals / Non-Goals

**Goals:**
- Settings window always shows current persisted state when opened (never unsaved form values, never post-sign-out account info).
- After sign-out, opening settings presents a clean, reloaded page.
- After cancelling wizard sign-in, the `step-welcome` UI is clean and contains no residual URL or error text.
- Newly created windows have a sensible minimum inner size (640x480).

**Non-Goals:**
- Real-time live-update of the settings window while it is open and visible (polling or Tauri events pushing mount status changes into an already-visible settings window).
- Wizard multi-step undo/back navigation redesign.
- Hot-reload of settings without page-level refresh (data-binding framework).

## Decisions

### D1: Emit `window-shown` event vs. `win.eval()` for settings refresh

**Options considered:**

A) Call `win.eval("loadSettings(); loadMounts();")` from Rust inside `open_or_focus_window` immediately before `win.show()`.

B) Emit a Tauri event (`window-shown`) and have JS listen for it, calling `loadSettings()` + `loadMounts()` on receipt.

**Decision: Option A (`win.eval()`).**

Rationale: `win.eval()` is synchronous from the Rust side — the JS is scheduled to execute in the webview before the call returns, which means it runs before the user sees the window. Option B (event-based) involves async listener setup and possible race conditions: the event could fire before the listener is registered if the webview's JS event infrastructure is not yet ready. `win.eval()` is already used in the codebase (`app-polish` change history) and is the simplest correct approach for this use case. The tradeoff is that `win.eval()` ties the Rust code to JS function names, but those names (`loadSettings`, `loadMounts`) are stable UI contract functions, not implementation details.

For the settings window specifically: eval `"loadSettings(); loadMounts();"` before every `win.show()`.

For the wizard window: no eval needed on ordinary show — the wizard's `init()` already runs at page-load. Reload (via `win.reload()`) is the correct mechanism when re-entering from sign-out, which `sign_out` already does for the wizard.

### D2: Sign-out settings window treatment — `reload()` vs. `hide()` + eval-on-show

**Options considered:**

A) Keep `hide()` in `sign_out`, rely on the eval-on-show from D1 to refresh on next open.

B) Call `reload()` on the settings window during `sign_out`, same as the wizard.

**Decision: Option B (`reload()`).**

Rationale: After sign-out the account display is fundamentally different (signed-out state, no account email, empty mount list). A `reload()` ensures the page starts from a completely clean state with no leftover DOM, including the active tab selection reset to General. The eval-on-show approach (Option A) would still show the old account display for a brief moment during the window's repaint cycle between `show()` and the JS executing. `reload()` avoids that flicker entirely. The cost — a slightly longer page-load time on next open — is negligible for a settings page.

### D3: Wizard `cancelSignIn` cleanup — DOM reset in JS vs. full `reload()`

**Options considered:**

A) Call `win.reload()` on the wizard from Rust when cancel is detected (requires a Tauri command or event from JS → Rust → JS).

B) Clear the stale fields in the existing `cancelSignIn()` JS function: set `auth-url` value to `""`, hide the `auth-error` div.

**Decision: Option B (local DOM reset in JS).**

Rationale: The wizard `cancelSignIn()` already manages local state (`signingIn = false`, `cleanupListeners()`). A full `reload()` from Rust would be over-engineered for what is a 2-line DOM cleanup. The stale auth URL and error message are scoped to the `step-signing-in` div; resetting them in `cancelSignIn()` keeps the fix local to the JS function that owns the state. Option A would add a Rust→JS round trip and complicate the flow.

### D4: Minimum window size

Add `min_inner_size(640.0, 480.0)` to the `WebviewWindowBuilder` chain in `open_or_focus_window`. This is purely additive and has no behavioral risk.

## Risks / Trade-offs

- **`win.eval()` silent failure**: If the window's webview is not yet ready when `eval()` is called, the JS executes but may fail silently (e.g., `invoke` not yet set up). Mitigation: `open_or_focus_window` only reaches the `eval` path for already-constructed windows that have previously loaded their HTML, so the webview is always initialized by the time `eval` runs.

- **`reload()` on settings clears user's current tab position**: If the user has the Advanced tab selected and sign-out fires from the tray menu (not the Account tab's Sign Out button), the settings window reloads to the General tab. This is acceptable — the user signed out, so the previous UI context is irrelevant.

- **Function name coupling**: `win.eval("loadSettings(); loadMounts();")` hard-codes JS function names in Rust. If these functions are renamed in the HTML, the Rust call silently becomes a no-op. Mitigation: these are documented public UI functions in a small, single-file HTML; the risk is low and will be caught in manual testing.

## Migration Plan

No migration needed. All changes are within the Tauri app layer and take effect on the next application build. No data format changes, no config changes, no user-visible breaking changes.

## Open Questions

None. The approach is fully determined.
