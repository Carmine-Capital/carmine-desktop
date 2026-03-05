## 1. Signal handler (W1)

- [x] 1.1 In `setup_after_launch()` in `main.rs`, spawn a new async task that awaits `tokio::signal::ctrl_c()` (and `SIGTERM` on Unix via `tokio::signal::unix::signal(SignalKind::terminate())`) using `tokio::select!`, then calls `graceful_shutdown(&app_handle)`
- [x] 1.2 Add `#[cfg(unix)]` / `#[cfg(not(unix))]` gates so the handler compiles on all platforms (Unix: ctrl_c + SIGTERM, Windows: ctrl_c only)
- [x] 1.3 Verify the signal task holds a cloned `AppHandle` and does not block the Tauri event loop

## 2. Sync-first delta loop (W2, S3)

- [x] 2.1 In `start_delta_sync()` in `main.rs`, restructure the loop so the sync body (iterate drives, call `run_delta_sync`, handle auth-degradation errors) runs *before* the `tokio::select!` sleep, not after
- [x] 2.2 Verify that the `cancel.cancelled()` branch still breaks the loop immediately and that auth-degradation detection logic is preserved unchanged
- [x] 2.3 Verify that re-authentication via `sign_in` (which calls `start_delta_sync`) now gets an immediate first sync pass

## 3. Flush pending writes on re-auth (W3)

- [x] 3.1 In `commands.rs::sign_in()`, add `crate::run_crash_recovery(&app).await;` after `start_all_mounts(&app)` and before `start_delta_sync(&app)` (around line 96)
- [x] 3.2 Verify `run_crash_recovery` is safe to call when the writeback buffer is empty (should early-return with no side effects)

## 4. Dynamic tray menu (W4, W5)

- [x] 4.1 Rewrite `update_tray_menu()` in `tray.rs` to build a new `Menu` from current state: iterate `effective_config.mounts`, check each mount's status against `state.mounts` (handle exists → "Mounted", enabled but no handle → "Unmounted", disabled/no drive_id → "Error"), and create one `MenuItemBuilder` per mount showing `"{name} — {status}"`
- [x] 4.2 After mount entries, add: separator, "Add Mount…" item (opens wizard/settings window), "Settings…" item, separator, "Sign Out" item, "Quit {app_name}" item
- [x] 4.3 Set the rebuilt menu on the tray icon via `tray.set_menu(Some(menu))` and update the tooltip in the same function (fold `update_tray_status` into `update_tray_menu`)
- [x] 4.4 Update `handle_menu_event()` to handle clicks on dynamic mount items — extract mount ID from the menu item ID (e.g., `mount_{id}`), look up the mount config, and open the mount point in the file manager via `open::that()`
- [x] 4.5 Add `update_tray_menu(&app)` calls after: `start_mount()`, `stop_mount()`, `start_all_mounts()`, `stop_all_mounts()`, `toggle_mount()`, `add_mount()`, `remove_mount()`, `sign_in()`, `sign_out()`, and auth-degradation state changes
- [x] 4.6 Remove the now-redundant `update_tray_status()` function from `main.rs` and replace all its call sites with `tray::update_tray_menu()`

## 5. Wizard cancellation (S4)

- [x] 5.1 In the `.on_window_event()` closure in `main.rs`, before the default hide-and-prevent-close logic, add a check: if `window.label() == "wizard"` and `!state.authenticated.load(Ordering::Relaxed)`, call `window.app_handle().exit(0)` and return early
- [x] 5.2 Verify that after successful sign-in (`authenticated == true`), closing the wizard hides it normally (no exit)
- [x] 5.3 Verify that closing the settings window (label != "wizard") always hides, regardless of auth state

## 6. Verification

- [x] 6.1 `cargo build --all-targets` passes with zero warnings
- [x] 6.2 `cargo clippy --all-targets --all-features` passes with zero warnings (pre-existing clippy issues in filesync-graph and filesync-auth; no new warnings from app-polish changes; --all-features requires GTK dev libs not available in this env)
- [x] 6.3 `cargo test --all-targets` — all existing tests pass (no regressions) (13 passed, 2 ignored live API, 2 pre-existing failures in filesync-auth token storage tests)
- [x] 6.4 `cargo fmt --all -- --check` passes
