## Context

Three UI interactions are silently broken in the current implementation:

1. **Sign Out / Remove Mount buttons do nothing** — `settings.js` uses `window.confirm()` for destructive action confirmation. Tauri 2 webviews block native JS dialog functions (`alert`, `confirm`, `prompt`) on some WebKit configurations; they return `undefined` silently. Since `undefined` is falsy, `if (!confirm(...)) return;` always returns early.

2. **Add Mount (settings Mounts tab) shows sign-in screen** — `addMount()` calls `invoke('open_wizard')`, which creates a fresh wizard window starting at `step-welcome`. The wizard has no mechanism to detect that the user is already authenticated.

3. **Add Mount (tray menu) shows sign-in screen** — same root cause. `handle_menu_event("add_mount")` calls `open_or_focus_window(app, "wizard", …)`, which either creates a new window at `step-welcome` or focuses an existing one stuck at `step-welcome`.

The Rust tray sign-out already works correctly because it uses `tauri_plugin_dialog` on the backend. `tauri-plugin-dialog` is already a workspace dependency.

## Goals / Non-Goals

**Goals:**
- Restore Sign Out button functionality in the Settings Account tab
- Restore Remove Mount button functionality in the Settings Mounts tab
- Make Add Mount (settings and tray) open the wizard at `step-sources` when the user is already authenticated

**Non-Goals:**
- Redesigning the wizard UX or its step flow
- Fixing other settings actions (save, toggle, etc.)
- Handling multi-account scenarios

## Decisions

### D1: Use `window.__TAURI__.dialog.confirm()` for destructive confirmations

**Chosen**: Replace `window.confirm()` in `settings.js` with `window.__TAURI__.dialog.confirm()` (Tauri plugin dialog JS API). Add `dialog:allow-confirm` to `capabilities/default.json`.

**Rationale**: The Tauri dialog plugin is already installed (`tauri-plugin-dialog` is a workspace dep, used in Rust for tray sign-out). Its JS API is reliable across all Tauri webview backends. Adding one capability entry costs nothing.

**Alternatives considered**:
- Custom in-page modal (e.g., `<dialog>` element) — works without capability changes, but adds bespoke HTML/CSS for something OS dialogs handle better
- Move confirmation to Rust backend — the `sign_out` command could refuse to proceed without its own dialog, but that breaks the frontend's ability to show error state and adds coupling

### D2: Add a lightweight `is_authenticated` Tauri command

**Chosen**: New `#[tauri::command] fn is_authenticated(app) -> bool` that reads `AppState.authenticated` with `Ordering::Relaxed`. Costs one atomic load per wizard open.

**Rationale**: The wizard needs to know auth state synchronously-ish on load, without a network call. `get_drive_info` would work but makes a Graph API request — wrong layer for a routing decision.

**Alternatives considered**:
- URL query parameter (`wizard.html?mode=add-mount`) — the tray would need to pass the param; and re-focusing an existing window doesn't change its URL, so the eval path would still be needed anyway
- Reuse `get_settings` — returns `account_display`, but a `None` display doesn't mean unauthenticated (account metadata can be absent even when tokens are valid)

### D3: Wizard detects auth state on load and routes to `step-sources`

**Chosen**: In `wizard.js init()`, call `invoke('is_authenticated')`. If `true`, call `onSignInComplete()` which transitions to `step-sources` and loads drive info / followed sites. No change to `init()`'s event wiring.

**Rationale**: This handles both the fresh-window case (new wizard opened for add-mount) and the case where the wizard is already loaded in `step-welcome` (re-focus via eval). It is the only routing path that works for all callers without passing arguments through the window system.

### D4: Navigate existing wizard window via `win.eval("goToAddMount()")`

**Chosen**: In `open_or_focus_window` (tray.rs), for the `wizard` label, if the window already exists and the caller is the `add_mount` path, call `win.eval("goToAddMount()")` before focusing. `goToAddMount()` is a new exported global function in `wizard.js` that wraps `onSignInComplete()`.

**Rationale**: The wizard window may still be open at `step-welcome` (e.g., the user opened sign-in but didn't complete it, or closed the success screen and re-opened). Simply focusing would leave it at the wrong step. Eval-calling `goToAddMount()` is the existing pattern already used for settings (`"loadSettings(); loadMounts()"`).

A new `open_or_focus_wizard` helper in `tray.rs` accepts a `mode: &str` parameter (`"sign_in"` vs `"add_mount"`) to keep the dispatch clean.

## Risks / Trade-offs

- **`window.__TAURI__.dialog.confirm()` is async** — `signOut()` and `removeMount()` are already `async`, so `await` drops in naturally. No sync/async mismatch risk.
- **`is_authenticated` race** — the atomic read is best-effort. If a sign-out races with wizard open, the wizard might show `step-sources` then fail on `get_drive_info`. This is benign: the error UI in `step-sources` handles it, and the race window is tiny in practice.
- **`goToAddMount()` called on unauthenticated wizard** — if somehow called while not authenticated, `loadSources()` calls `get_drive_info` / `get_followed_sites` which fail gracefully and show the error state in `step-sources`. Not a crash.
