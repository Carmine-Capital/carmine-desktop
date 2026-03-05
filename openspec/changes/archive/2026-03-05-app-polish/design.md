## Context

All six FileSync crates are implemented and tested. The `app-runtime-orchestration` change wired them together — `AppState` holds `AuthManager`, `GraphClient`, `CacheManager`, mount handles, and delta-sync cancellation. Sign-in, sign-out, mount start/stop, delta sync, crash recovery, and graceful shutdown all function.

A post-implementation verification pass found 5 warnings (spec violations) and 4 suggestions (robustness improvements). This change addresses them as a focused polish pass before front-end work begins.

**Current gaps (code references):**

| ID | File | Line(s) | Gap |
|----|------|---------|-----|
| W1 | `main.rs` | `setup_after_launch` | No `tokio::signal` handler — Ctrl+C / `kill` skips `graceful_shutdown()` |
| W2 | `main.rs` | 434–438 | Delta sync loop sleeps before first run — stale cache for up to 60 s after re-auth |
| W3 | `commands.rs` | 94–96 | `sign_in` does not call `run_crash_recovery()` — pending writes from degraded mode stay in buffer |
| W4 | `tray.rs` | 99–117 | `update_tray_menu()` updates tooltip only, does not rebuild menu items |
| W5 | `tray.rs` | 13–46 | Static menu: no per-mount entries, no "Add Mount…" |
| S3 | `main.rs` | 434 | First delta sync after startup also delayed by full interval |
| S4 | `main.rs` | 210–215 | `on_window_event` hides all windows on close — wizard cancel during first-run doesn't exit |

## Goals / Non-Goals

**Goals:**

- Every spec scenario tagged to these gaps passes: signal shutdown, re-auth sync+flush, dynamic tray menu, wizard cancellation.
- Zero new crate dependencies.
- Zero changes to config format, public types, or crate API surfaces.
- All changes confined to `filesync-app` (except an optional `DeltaSyncTimer` enhancement in `filesync-cache`).

**Non-Goals:**

- Tray icon state indicators (green/syncing/error) — requires platform-specific icon assets; separate change.
- DeltaSyncTimer error-callback refactor (S1) — the current manual loop is justified by auth-degradation logic; defer unless the duplication becomes a maintenance burden.
- Positive-path token restoration test (S2) — requires OS keyring mocking; out of scope.
- Headless-mode signal handling — headless mode is a stub; will be addressed when headless mode is fully implemented.

## Decisions

### D1: Signal handler as a spawned task in `setup_after_launch()`

Register signal handlers inside the existing `setup_after_launch()` async function, which already runs as a spawned task after Tauri `.setup()`.

```
tokio::spawn(async move {
    #[cfg(unix)] {
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))] {
        tokio::signal::ctrl_c().await.ok();
    }
    graceful_shutdown(&app_handle);
});
```

**Why not VFS's `shutdown_on_signal()`?** That function operates on raw `Vec<MountHandle>`, not app-level `AppState`. It doesn't cancel the delta sync timer or update tray state. Reusing it would require refactoring its signature for marginal gain — the app-level handler is 12 lines.

**Alternative considered**: Register in Tauri's `.setup()` closure directly. Rejected because `.setup()` is sync and `tokio::signal` is async — we'd need `tauri::async_runtime::spawn` anyway, which is what `setup_after_launch` already does.

### D2: Sync-first delta loop

Restructure `start_delta_sync()` so the loop body runs *before* the first sleep:

```
loop {
    // Run sync first
    for drive_id in &drives { ... }
    // Then sleep
    tokio::select! {
        _ = cancel.cancelled() => break,
        _ = tokio::time::sleep(wait) => {}
    }
}
```

This fixes both W2 (re-auth delay) and S3 (startup delay) with a single structural change. The existing auth-degradation detection inside the loop body is preserved unchanged.

**Alternative considered**: Running a one-shot sync before entering the loop. Rejected — it duplicates the loop body and the auth-degradation handling.

### D3: Flush pending writes on re-authentication

Add a single call `run_crash_recovery(&app).await` in `commands.rs::sign_in()` between `start_all_mounts` and `start_delta_sync`:

```rust
crate::start_all_mounts(&app);
crate::run_crash_recovery(&app).await;  // W3 fix
crate::start_delta_sync(&app);
```

`run_crash_recovery` already exists and does exactly the right thing — iterates pending writes and uploads them. It's safe to call when the buffer is empty (early-returns). Placing it after `start_all_mounts` ensures the `GraphClient` has valid tokens and drive IDs are populated.

### D4: Dynamic tray menu rebuild

Replace the current `update_tray_menu()` (tooltip-only) with a full menu reconstruction:

1. Read mount configs from `effective_config` and active mount handles from `mounts`.
2. For each mount config: create a `MenuItemBuilder` showing `"{name} — {status}"` where status is `Mounted` (handle exists), `Unmounted` (enabled but no handle), or `Error` (disabled/missing drive_id). Clicking opens the mount folder in the file manager.
3. After the mount list: separator, "Add Mount…" (opens wizard/settings), "Settings…", separator, "Sign Out", "Quit {app_name}".
4. Set the new menu on the tray icon via `tray.set_menu(Some(menu))`.

**Call sites** — `update_tray_menu()` is called after every mount-state change:
- `start_mount()` success
- `stop_mount()` success
- `start_all_mounts()` completion
- `stop_all_mounts()` completion
- `toggle_mount()` completion
- `add_mount()` completion
- `remove_mount()` completion
- `sign_in()` completion
- `sign_out()` completion
- Auth degradation state change

`update_tray_status()` is folded into `update_tray_menu()` since the tooltip update is a subset of the menu rebuild.

**Alternative considered**: Patching individual menu items instead of rebuilding. Rejected — Tauri's menu API makes full rebuild simpler and the menu has <10 items; performance is not a concern.

### D5: Wizard cancellation exits on first-run

Modify `on_window_event` to check if the closing window is the wizard during `first_run`:

```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        if window.label() == "wizard" {
            // Check if we're still in first-run (not yet authenticated)
            if let Some(state) = window.app_handle().try_state::<AppState>() {
                if !state.authenticated.load(Ordering::Relaxed) {
                    // First-run wizard cancelled — exit without config
                    window.app_handle().exit(0);
                    return;
                }
            }
        }
        let _ = window.hide();
        api.prevent_close();
    }
})
```

The key insight: after sign-in completes, `authenticated` is `true`, so closing the wizard post-auth correctly hides instead of exiting. Only pre-auth wizard closure triggers exit.

**Alternative considered**: A dedicated `first_run` flag on `AppState`. Rejected — `!authenticated` already encodes the same state for the wizard case, and avoids adding another atomic bool.

## Risks / Trade-offs

- **[Risk] Menu rebuild on every mount change** → The menu has at most ~10 items (typical: 2–3 mounts + 4 fixed items). Tauri menu construction is synchronous and fast. No mitigation needed.
- **[Risk] Signal handler race with Tauri event loop** → `graceful_shutdown()` calls `app.exit(0)` which is thread-safe in Tauri. The spawned signal task holds a cloned `AppHandle`. No race condition.
- **[Risk] `run_crash_recovery` called twice on startup+re-auth** → Safe: if the buffer is empty (already flushed on startup), `run_crash_recovery` returns immediately. Second call is a no-op.
- **[Trade-off] Wizard exit check uses `!authenticated` instead of a `first_run` flag** → Slightly overloaded semantics: if a signed-out user closes the wizard, the app exits too. This matches the spec ("wizard cancellation: exit cleanly without creating any configuration") since sign-out reopens the wizard precisely to force re-authentication.
