## 1. Bool-based mount status tracking

- [x] 1.1 In `update_tray_menu()` (`tray.rs:135`), change the `entries` Vec type from `Vec<(String, String, String)>` to `Vec<(String, String, bool)>` where the third field is `is_mounted = active_mounts.contains_key(&mc.id)`
- [x] 1.2 Remove the now-redundant `mc.id.clone()` as the third tuple element; the mount ID is already encoded in the item ID via the `"mount_"` prefix
- [x] 1.3 Replace the substring-matching tooltip count (`label.contains("Mounted") && !label.contains("Unmounted")`, `tray.rs:168`) with `mount_entries.iter().filter(|(_, _, is_mounted)| *is_mounted).count()`
- [x] 1.4 Update the destructuring of `mount_entries` in the menu-builder loop (`tray.rs:182`) to match the new `(item_id, label, _)` shape

## 2. Auth-degraded "Re-authenticate" menu item

- [x] 2.1 In `update_tray_menu()` (`tray.rs:218-224`), when `auth_degraded == true`, add a `MenuItemBuilder::with_id("re_authenticate", "Re-authenticate\u{2026}")` item to the builder immediately before the "Sign Out" item
- [x] 2.2 In `handle_menu_event()` (`tray.rs:69-103`), add a `"re_authenticate"` match arm that calls `open_or_focus_window(app, "wizard", "Setup", "wizard.html")`

## 3. Left-click routing based on auth state

- [x] 3.1 In the `on_tray_icon_event` closure in `setup()` (`tray.rs:41-50`), replace the unconditional `open_or_focus_window(..., "settings", ...)` call with a branch: read `app_handle.try_state::<crate::AppState>()`, load `authenticated.load(Ordering::Relaxed)`, and open the wizard when `false` or settings when `true`

## 4. Verification

- [x] 4.1 Run `cargo clippy --all-targets --all-features` and confirm zero warnings
- [x] 4.2 Run `cargo fmt --all -- --check` and confirm no formatting issues
- [ ] 4.3 Manual test — unauthenticated state: left-click opens wizard, tray menu shows "Sign In…" with no "Re-authenticate…" item
- [ ] 4.4 Manual test — authenticated state: left-click opens settings, tooltip correctly counts mounted drives even for mount names containing the word "Mounted" or "Unmounted"
- [ ] 4.5 Manual test — auth degraded state: tooltip says "Re-authentication required", menu shows "Re-authenticate…" before "Sign Out", clicking "Re-authenticate…" opens the wizard
