## Why

Silent failures and missing confirmations across all async and destructive operations in the settings UI erode user trust and risk unintended data loss — a user can accidentally sign out or remove a mount with a single mis-click, with no indication that anything happened (or went wrong). This inconsistency is especially jarring because `clearCache()` is the sole operation that already has feedback (via `alert()`), making the absence everywhere else more conspicuous.

## What Changes

- **Toast notification system**: add a lightweight in-page status bar (HTML/CSS, no external library) shown below the action area; replaces the single `alert()` in `clearCache()` for consistency.
- **Loading state on action buttons**: disable the triggering button and update its label while an async invoke is in flight (e.g., "Saving…", "Removing…", "Signing out…").
- **Success toasts**: `saveGeneral`, `saveAdvanced`, `toggleMount`, `removeMount`, `signOut` (settings), and `clearCache` all show a brief "Saved", "Done", etc. message on success.
- **Error toasts**: every `catch` block surfaces the error to the user instead of swallowing it to `console.error`.
- **Confirmation before remove mount**: `removeMount` shows a `confirm()` dialog ("Remove this mount? This cannot be undone.") before invoking the backend.
- **Confirmation before sign-out (settings)**: `signOut` in settings shows a `confirm()` dialog ("Sign out? All mounts will stop.") before proceeding.
- **Confirmation before sign-out (tray)**: the `"sign_out"` branch in `tray.rs` `handle_menu_event` opens a Tauri dialog before calling `commands::sign_out`, so the tray path has the same protection as the settings path.

## Capabilities

### New Capabilities

- `ui-feedback`: In-page toast/status notification system for settings.html; loading states on action buttons; success and error feedback for all async operations; confirmation dialogs for destructive actions (remove mount, sign-out from settings and tray).

### Modified Capabilities

- `tray-app`: The sign-out action (tray menu and Account tab) now **requires** a user confirmation before executing; and the Account tab sign-out and all settings save/toggle/remove operations now display success or error feedback to the user. These are spec-level behavioral changes to existing scenarios.

## Impact

- `crates/carminedesktop-app/dist/settings.html`: all JavaScript functions (`saveGeneral`, `saveAdvanced`, `toggleMount`, `removeMount`, `signOut`, `clearCache`) plus new CSS and toast/status DOM.
- `crates/carminedesktop-app/src/tray.rs`: `handle_menu_event` `"sign_out"` branch — add Tauri `dialog::blocking::confirm` (or async equivalent) before spawning `sign_out`.
- No new Rust dependencies required if using Tauri's built-in dialog plugin (`tauri-plugin-dialog`); confirm that it is already in scope or add it to workspace dependencies.
- No changes to backend commands, cache, Graph, or VFS layers.
