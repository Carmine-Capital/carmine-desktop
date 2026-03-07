## 1. Harden `update_tray_menu` against poisoned mutexes

- [x] 1.1 In `update_tray_menu` (`tray.rs`), replace `app_state.effective_config.lock().unwrap()` with `if let Ok(config) = app_state.effective_config.lock() { ... } else { tracing::warn!(...); return; }`
- [x] 1.2 In the same block, replace `app_state.mounts.lock().unwrap()` with `if let Ok(active_mounts) = app_state.mounts.lock() { ... } else { tracing::warn!(...); return; }`
- [x] 1.3 In the `pending_update_version` computation, replace `.and_then(|s| s.pending_version.lock().unwrap().clone())` with `.and_then(|s| s.pending_version.lock().ok().and_then(|g| g.clone()))` to handle poisoned mutex without panicking

## 2. Add `sign_out_failed` notification

- [x] 2.1 In `notify.rs`, add a `pub fn sign_out_failed(app: &AppHandle, reason: &str)` function following the existing `send()` helper pattern

## 3. Restructure `sign_out` to guarantee UI reset

- [x] 3.1 In `commands.rs` `sign_out`, replace the `?` propagation chain with explicit error collection: wrap `stop_all_mounts`, `auth.sign_out().await`, `user_config` block, and `rebuild_effective_config` into a phase-1 block that accumulates errors into a `Vec<String>` rather than returning early on first error
- [x] 3.2 After phase 1, unconditionally execute phase 2: `state.authenticated.store(false, Ordering::Relaxed)`, `state.auth_degraded.store(false, Ordering::Relaxed)`, `crate::tray::update_tray_menu(&app)`, reload settings window, show wizard (the existing code from lines 194–208)
- [x] 3.3 After phase 2, if phase-1 errors is non-empty, call `crate::notify::sign_out_failed(&app, &errors.join("; "))` and return `Err(errors.join("; "))`

## 4. Verification

- [x] 4.1 Run `cargo clippy --all-targets --all-features` and confirm zero warnings
- [x] 4.2 Run `cargo fmt --all -- --check` and confirm no formatting issues
- [ ] 4.3 Manual test — sign out while a mount is active: confirm drive unmounts, tray transitions to "Sign In…", and wizard appears on first click (no second click required)
- [ ] 4.4 Manual test — tray menu updates correctly after sign-out: confirm menu shows "Sign In…" (not "Sign Out") when opened after the first sign-out attempt

## 5. Fix `flush_pending` block_on panic (actual root cause)

The real panic is `flush_pending` calling `self.rt.block_on()` from inside a Tokio worker thread
(sign_out is async/spawned). `Handle::block_on` panics in async contexts. The file already uses
`tokio::task::block_in_place(|| rt.block_on(...))` at line 100 — apply the same pattern here.

- [x] 5.1 In `flush_pending` (`mount.rs`), wrap the first `self.rt.block_on(self.cache.writeback.list_pending())` with `tokio::task::block_in_place(|| ...)`
- [x] 5.2 In `flush_pending` (`mount.rs`), wrap the second `self.rt.block_on(async { ... })` with `tokio::task::block_in_place(|| ...)`
- [x] 5.3 Run `cargo clippy --all-targets` and `cargo fmt --all -- --check`
