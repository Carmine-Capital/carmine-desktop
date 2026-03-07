## Why

Sign-out from the tray menu silently fails on the first attempt: the drive is unmounted but the app remains in an authenticated state (tray still shows "Sign Out", wizard never opens). The user must click "Sign Out" a second time to complete the flow. The root cause is a panic in `update_tray_menu` — triggered from within `stop_mount` during `stop_all_mounts` — that aborts the Tokio task before `authenticated.store(false)`, `update_tray_menu(auth=false)`, and wizard-open are reached. Panics inside `tokio::spawn` tasks are silently swallowed and are not caught by the `if let Err(e) = sign_out(...).await` guard in the tray event handler.

## What Changes

- **Replace `unwrap()` with graceful error handling** in `update_tray_menu`: the three `.unwrap()` calls on `effective_config`, `mounts`, and `UpdateState::pending_version` are converted to `?`-style or `if let Ok` patterns so that a poisoned mutex logs a warning rather than aborting the Tokio task.
- **Make `sign_out` resilient to partial failure**: extract the state-clearing and UI-recovery steps (`authenticated.store(false)`, `update_tray_menu`, wizard open) into an always-runs block so they execute even if an earlier step (e.g., `rebuild_effective_config`) returns `Err`.
- **Surface errors to the user**: if sign_out fails for any reason other than the user cancelling, emit a desktop notification so the failure is not invisible.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `tray-app`: sign-out behavior requirements change — `update_tray_menu` must not panic on poisoned mutexes, sign-out must always transition to the unauthenticated tray state, and sign-out errors must be surfaced via notification.
- `app-lifecycle`: sign-out lifecycle requirement changes — the authenticated flag and wizard-open step must execute even when intermediate sign-out steps fail.

## Impact

- `crates/cloudmount-app/src/tray.rs` — `update_tray_menu`: replace `unwrap()` with graceful handling
- `crates/cloudmount-app/src/commands.rs` — `sign_out`: restructure to always execute state reset and UI recovery
- `openspec/specs/tray-app/spec.md` (delta) — sign-out robustness requirements
- `openspec/specs/app-lifecycle/spec.md` (delta) — sign-out lifecycle requirements
