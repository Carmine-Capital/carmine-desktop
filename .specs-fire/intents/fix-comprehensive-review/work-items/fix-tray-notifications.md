---
id: fix-tray-notifications
title: Fix dead tray items, notification click actions, win.eval to events
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-019
completed_at: 2026-03-09T19:25:03.465Z
---

# Work Item: Fix dead tray items, notification click actions, win.eval to events

## Description

Fix tray menu and notification issues:

1. **open_folder dead** (`tray.rs:76`): "Open Mount Folder" menu item defined in setup but `handle_menu_event` has no case for `"open_folder"` — falls through to `_ => {}`. Fix: either remove the static item (it's replaced by per-mount items in `update_tray_menu`) or add a handler that opens the first mount folder.

2. **Re-auth notification not clickable** (`notify.rs:65`): `auth_expired()` says "Click to re-authenticate" but has no `.on_click()` handler. Fix: use Tauri notification action API to open the wizard with re-auth flow, or remove the misleading "Click to" text.

3. **win.eval bypasses CSP** (`tray.rs:138,156`): `win.eval("goToAddMount()")` and `win.eval("loadSettings(); loadMounts();")` rely on Tauri internals. Fix: use Tauri events — `app.emit("navigate-to-add-mount", ())` from Rust, listen with `window.__TAURI__.event.listen("navigate-to-add-mount", ...)` in JS.

4. **Update check silent** (`update.rs:144`): Manual "Check for Updates" failure only logs `tracing::warn`. Fix: add `notify::update_check_failed()` notification or show error via tray status.

5. **Linux tray left-click** (`tray.rs:35-57`): AppIndicator backend on Linux may not fire `TrayIconEvent::Click`. This is a known Tauri limitation. Fix: document in code comment; ensure all functionality is accessible via right-click menu (it already is).

## Acceptance Criteria

- [ ] "Open Mount Folder" either has a working handler or is removed from static menu
- [ ] auth_expired notification either has click action or text changed to remove "Click to"
- [ ] Tray-to-webview communication uses Tauri events instead of win.eval()
- [ ] Manual update check failure shows user-visible notification
- [ ] Linux tray limitation documented in code comment
- [ ] JS files have event listeners for new Tauri events

## Technical Notes

For Tauri events pattern: Rust emits `app.emit("event-name", payload)`, JS listens with `window.__TAURI__.event.listen("event-name", callback)`. This is CSP-safe and doesn't depend on Tauri internals.

For notification clicks, Tauri v2's notification plugin supports `actionTypeId` and `registerActionTypes`. Check if the desktop feature includes the notification action plugin. If not, remove "Click to" from the text as the simpler fix.

## Dependencies

(none)
