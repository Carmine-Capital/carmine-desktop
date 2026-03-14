## Context

`settings.html` is a self-contained Tauri webview page (~195 lines). All async operations (`saveGeneral`, `saveAdvanced`, `toggleMount`, `removeMount`, `signOut`, `clearCache`) call Tauri `invoke()` but swallow errors silently — only `clearCache` surfaces anything, via the native `alert()` API. The tray's `"sign_out"` branch in `tray.rs` `handle_menu_event` runs immediately on menu click with no confirmation gate.

The tray currently has no dependency on `tauri-plugin-dialog`. The `confirm()` global available in browser JS works inside Tauri webviews, but native OS dialogs from the Rust side require the dialog plugin.

## Goals / Non-Goals

**Goals:**
- Every async operation in settings.html gives visible feedback: loading state while in flight, success confirmation on completion, error message on failure.
- Destructive operations (remove mount, sign-out from settings, sign-out from tray) require explicit user confirmation before proceeding.
- The existing `alert()` in `clearCache` is replaced with the same toast mechanism for consistency.
- The tray sign-out path gets a native OS confirmation dialog via `tauri-plugin-dialog`.

**Non-Goals:**
- Redesigning the settings UI layout or tab structure.
- Adding undo/redo for destructive operations.
- Persistent notifications or a notification history log.
- Localisation / i18n of message strings.
- Any changes to wizard.html (out of scope for this change; wizard flows have their own feedback model).

## Decisions

### D1: In-page toast bar, not native OS dialogs, for success/error feedback

A single `<div id="status-bar">` is positioned at the bottom of the settings window. CSS transitions handle fade-in/out. A `showStatus(message, type)` helper sets the text and triggers the animation via a class toggle, then auto-hides after 3 seconds via `setTimeout`.

**Alternatives considered:**
- **Native OS notification** via `tauri-plugin-notification`: overkill for inline feedback; fires into the OS tray area, which is wrong for settings-window feedback.
- **Third-party toast library** (Toastify, etc.): no external JS dependencies allowed; the existing file has none and is self-contained.
- **`alert()`**: already present in `clearCache` but is modal and blocking — poor UX for simple status messages.

### D2: `confirm()` for destructive actions in settings.html (JS side)

`removeMount` and `signOut` (settings) use the synchronous `window.confirm()` built-in before calling `invoke()`. This is acceptable inside a Tauri webview and requires no additional plugin.

**Alternatives considered:**
- **Custom modal dialog in HTML**: adds significant DOM/CSS complexity for a simple yes/no prompt; `confirm()` matches platform conventions without extra code.
- **Tauri `dialog` plugin JS API**: would require `tauri-plugin-dialog` on the frontend too; unnecessary when `confirm()` works.

### D3: `tauri-plugin-dialog` for the tray sign-out confirmation (Rust side)

The tray `handle_menu_event` function runs on the Rust side with no webview context. The cleanest approach is to call `tauri_plugin_dialog::DialogExt::dialog(&app).message("...").blocking_show()` (or the async variant inside the spawned task) to present a native OS confirmation before calling `commands::sign_out`.

**Alternatives considered:**
- **Open a minimal Tauri webview as a confirm dialog**: heavy; introduces a new window lifecycle for a one-off confirmation.
- **Route through the settings/wizard window JS**: requires the window to be open and focused; not reliable from a background tray event.
- **Skip confirmation on tray**: inconsistent with the spec requirement that sign-out always requires confirmation.

`tauri-plugin-dialog` must be added to `[workspace.dependencies]` in the root `Cargo.toml` and to the `desktop` feature list in `carminedesktop-app/Cargo.toml`.

### D4: Button loading state via `disabled` + text swap

When an async action starts, the calling button is found via `event.target` (or by ID for keyboard-triggered invocations), disabled, and its text replaced with a "…" variant. On resolve or reject the button is re-enabled and its original label restored. This prevents double-submission and signals to the user that work is in progress.

**Alternatives considered:**
- **CSS spinner overlay**: more visual but adds CSS complexity; the label swap is sufficient at this window size.

## Risks / Trade-offs

- **`confirm()` is synchronous and blocks the JS event loop**: this is fine for a settings window with no concurrent animations; the user must dismiss before anything else proceeds, which is the intended behaviour.
- **`tauri-plugin-dialog` adds a new dependency**: it is an official first-party Tauri plugin at the same version (`"2"`) as the other plugins already present, so version alignment is straightforward.
- **Toast auto-hide timing (3s)**: too short for long error messages. Mitigation: error toasts stay visible until the next action (do not auto-hide), so the user can read them.
- **`event.target` for button identity**: if the user manages to trigger the same action twice before the first resolves (e.g., via keyboard + mouse), the second call is blocked by the disabled state, so no double-invoke risk.

## Migration Plan

1. Add `tauri-plugin-dialog = "2"` to root `Cargo.toml` `[workspace.dependencies]`.
2. Add `tauri-plugin-dialog` to `carminedesktop-app`'s `desktop` feature and optional deps.
3. Register the dialog plugin in `main.rs` `setup_after_launch` alongside the other plugins.
4. Patch `tray.rs` `handle_menu_event` `"sign_out"` branch.
5. Patch `settings.html` with toast infrastructure and updated JS functions.

No database migrations, no config changes, no breaking changes to existing Tauri commands. Rollback: revert the five affected files.

## Open Questions

- Should the tray confirmation use `blocking_show` (simpler, synchronous) or the async `ask` API? Given the `sign_out` branch already spawns a `tauri::async_runtime` task, the async `ask` variant is more natural and avoids blocking the OS UI thread.
