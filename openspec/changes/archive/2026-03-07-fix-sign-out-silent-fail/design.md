## Context

When "Sign Out" is selected from the tray menu, `commands::sign_out` runs inside a `tauri::async_runtime::spawn` task. The tray handler catches only `Err` returns:

```rust
if let Err(e) = crate::commands::sign_out(app).await {
    tracing::error!("sign out failed: {e}");
}
```

Panics inside a spawned task are silently swallowed — they abort the task without returning `Err`. `sign_out` calls `stop_all_mounts`, which calls `stop_mount` for each active mount, which calls `update_tray_menu` after each unmount. `update_tray_menu` contains three `unwrap()` calls on `Mutex` guards:

```rust
let config = app_state.effective_config.lock().unwrap();   // panics if poisoned
let active_mounts = app_state.mounts.lock().unwrap();      // panics if poisoned
// and in the pending-update branch:
s.pending_version.lock().unwrap().clone()                  // panics if poisoned
```

If any of these mutexes are poisoned (e.g., from a prior panic in the update checker or FUSE background thread), `update_tray_menu` panics. The panic unwinds through `stop_mount` → `stop_all_mounts` → the spawned sign_out task, which Tokio silently drops. The drive IS unmounted (it happened before the panic), but `authenticated` is never set to `false`, `update_tray_menu` is never called with the unauthenticated state, and the wizard is never opened.

On the second click, there are no active mounts, so `stop_all_mounts` is a no-op and the panic is not triggered.

## Goals / Non-Goals

**Goals:**
- Remove `unwrap()` from `update_tray_menu` so a poisoned mutex logs a warning instead of aborting the caller's task
- Make the state-reset sequence in `sign_out` (set `authenticated = false`, call `update_tray_menu`, open wizard) always execute, even when earlier steps return `Err`
- Emit a desktop notification when sign-out encounters an error so the failure is not invisible

**Non-Goals:**
- Investigating or fixing the root cause of mutex poisoning (treated as a pre-existing instability; the fix makes sign-out survive it)
- Reworking `update_tray_menu` beyond replacing `unwrap()` with graceful handling
- Adding retry logic to sign-out

## Decisions

### D1: Replace `unwrap()` in `update_tray_menu` with graceful returns

`update_tray_menu` is `pub fn` called from many sites. Converting it to return `Result` would require all callers to handle errors. Instead, use `if let Ok(config) = app_state.effective_config.lock()` and early-return-with-log if the lock is poisoned. This keeps the call-site API unchanged and treats a poisoned lock as "skip this update silently" — which is correct, since a poisoned mutex indicates a prior panic and the tray state is already compromised.

*Alternative considered*: propagate errors to callers. Rejected because all call sites currently ignore the return value anyway, and adding `?` would require API changes across ~10 call sites.

### D2: Restructure `sign_out` to always execute the state-reset block

Split the sign_out body into two phases:
1. **Best-effort cleanup** (`stop_all_mounts`, `auth.sign_out()`, config save, `rebuild_effective_config`) — failures are logged but do not prevent phase 2
2. **Guaranteed UI reset** (`authenticated.store(false)`, `auth_degraded.store(false)`, `update_tray_menu`, reload settings window, open wizard) — always runs regardless of phase 1 errors

*Alternative considered*: wrap phase 1 in `let result = ...; if result.is_err() { ... }`. Rejected for a more direct restructuring: collect errors from phase 1 and propagate them to the user after phase 2, rather than short-circuiting with `?`.

### D3: Notify user on sign-out error via existing notification infrastructure

If phase 1 produced any errors, emit a `crate::notify::sign_out_failed` notification after the UI reset. This uses the existing `notify` module pattern and requires no new dependency.

## Risks / Trade-offs

- [Poisoned mutex in `update_tray_menu`] After D1, a poisoned `effective_config` or `mounts` mutex causes `update_tray_menu` to silently skip the update. The tray menu may show stale data. → Mitigation: log a `tracing::warn!` so operators can diagnose the issue. The fix prevents the crash; stale tray data is acceptable as a degraded state.
- [Phase-1 errors after split] With D2, partial phase-1 completion (e.g., auth tokens cleared but config not saved) is possible. On next launch, token restore may fail and the app shows the wizard — which is correct behavior for a signed-out state. No data loss risk.
- [Wizard focus on Linux] The wizard window may not steal focus on Linux due to WM focus-stealing prevention. This is a separate, known issue and is not addressed here.

## Migration Plan

No migration required. The fix is purely internal to `tray.rs` and `commands.rs`. No config schema, storage format, or public API changes.
