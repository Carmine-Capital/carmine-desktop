## Context

All three bugs live exclusively in `crates/cloudmount-app/src/tray.rs`. The file is ~240 lines and already holds the `AppState` atomics (`authenticated`, `auth_degraded`) that are needed for the fixes. No new state, no new crates, no schema migrations.

Current behavior:

- `setup()` (line 41-50): `show_menu_on_left_click(false)` + left-click handler unconditionally calls `open_or_focus_window(..., "settings", ...)` regardless of `authenticated`.
- `update_tray_menu()` (line 163-175): when `auth_degraded == true` the tooltip reflects the problem but the menu only offers "Sign Out" — no "Re-authenticate" shortcut.
- `update_tray_menu()` (line 135, 163-175): mount entries are stored as `Vec<(id: String, label: String, mount_id: String)>` where `label` encodes status as a display string. The tooltip derives mount count by filtering `label.contains("Mounted") && !label.contains("Unmounted")`, which breaks for mount names that happen to include those words.

## Goals / Non-Goals

**Goals:**
- Left-click routes to wizard when `authenticated == false`, settings otherwise.
- When `auth_degraded == true`, the tray menu includes a "Re-authenticate…" item that opens the wizard.
- Mount count in the tooltip is computed from an explicit boolean field, not string matching.

**Non-Goals:**
- Changing macOS left-click to always show the full menu (platform convention alignment can be addressed separately).
- Adding platform-specific `show_menu_on_left_click` overrides — the current cross-platform approach (left-click opens a window) is intentional and preserved.
- Any changes outside `tray.rs`.

## Decisions

### D1: Read `authenticated` atomic inside the left-click handler

The left-click handler in `setup()` runs in a closure that captures `app_handle`. `AppState` is available via `app_handle.try_state::<AppState>()`. The handler reads `authenticated.load(Ordering::Relaxed)` and branches:

- `false` → `open_or_focus_window(app, "wizard", "Setup", "wizard.html")`
- `true` → `open_or_focus_window(app, "settings", "Settings", "settings.html")` (current behavior)

Alternative considered: store a separate `AtomicBool` in `TrayState`. Rejected — `AppState.authenticated` is already the canonical source of truth and is available from any `AppHandle`.

### D2: "Re-authenticate…" item placement and behavior

When `auth_degraded == true`, a `MenuItemBuilder::with_id("re_authenticate", "Re-authenticate\u{2026}")` item is added to the menu immediately before "Sign Out". Its handler in `handle_menu_event` matches `"re_authenticate"` and calls `open_or_focus_window(app, "wizard", "Setup", "wizard.html")`.

"Sign Out" is kept alongside it — a user may genuinely want to switch accounts rather than just refresh the session. The new item appears first because re-authentication (without losing the account) is the lower-friction action.

Alternative considered: Replace "Sign Out" with "Re-authenticate…" when degraded. Rejected — removing "Sign Out" makes account-switch impossible from the tray in the degraded state, which is worse UX.

### D3: Bool-based mount status tracking

The `entries` Vec type changes from `Vec<(String, String, String)>` to `Vec<(String, String, bool)>` where:

- field 0: menu item ID (e.g., `"mount_<id>"`)
- field 1: display label (e.g., `"OneDrive — Mounted"`)
- field 2: `is_mounted: bool` — `true` iff `active_mounts.contains_key(&mc.id)`

The `status` string for the label is still derived the same way and still used for display. The bool is derived once during construction:

```rust
let is_mounted = active_mounts.contains_key(&mc.id);
```

The tooltip mount count then becomes:

```rust
let mounted = mount_entries.iter().filter(|(_, _, is_mounted)| *is_mounted).count();
```

This eliminates the substring dependency entirely. No other consumer of `mount_entries` is affected because the third field was previously an opaque `mc.id` clone used only for identification — that usage is removed (the item ID already encodes the mount ID via `"mount_{id}"` prefix).

## Risks / Trade-offs

- [Risk] `try_state::<AppState>()` in the left-click handler returns `None` before `AppState` is managed (race on startup). Mitigation: the handler already runs only after `setup()` which is called after `app.manage(state)` in `main.rs`; `None` → silently skip (no crash, user just gets no window).
- [Risk] The third field type change breaks any future code that stored the raw `mc.id` for lookup. Mitigation: the `mount_` prefix on the item ID already provides the ID; the third field was redundant. Any new code should strip `"mount_"` from the event ID, which is the existing pattern in `handle_menu_event`.

## Migration Plan

Single-file change, no persistent state, no config schema changes. No migration needed. The tray menu is rebuilt on every state change, so users see the new behavior immediately on the next `update_tray_menu()` call after the binary is updated.
