## Why

The app-runtime-orchestration change wired all 6 crates together but a verification pass revealed 9 behavioral gaps where the implementation diverges from the specs. Five are functional warnings — missing signal handling, stale data after re-auth, unflushed writes after re-auth, a static tray menu, and wizard cancellation not exiting — that directly violate `app-lifecycle`, `tray-app`, and `cache-layer` spec scenarios. Four are lower-priority suggestions that improve robustness (immediate first sync, DeltaSyncTimer reuse, positive-path token test, wizard close behavior). Addressing these now prevents the gaps from compounding as we build the frontend.

## What Changes

- **Signal handler registration** — spawn a task in `setup_after_launch()` that awaits `tokio::signal::ctrl_c()` (+ `SIGTERM` on Unix) and calls `graceful_shutdown()`, so `kill` and Ctrl+C perform ordered shutdown identical to Quit.
- **Immediate delta sync on re-auth** — after `sign_in` restarts mounts, run one `run_delta_sync` per drive *before* entering the timed loop, so stale cache data is refreshed within seconds instead of up to 60 s.
- **Pending-writes flush on re-auth** — call `run_crash_recovery()` inside `sign_in` after mounts start, so files written during auth-degraded mode upload immediately instead of waiting for the next app restart.
- **Dynamic tray context menu** — rebuild the tray `Menu` from current mount state (per-mount entries with status, "Add Mount…", "Settings…", "Sign Out", "Quit"), and call `update_tray_menu()` after every mount-state change.
- **Wizard cancellation exits cleanly** — detect when the wizard window is closed during `first_run` and call `app.exit(0)` instead of hiding the window, satisfying the "exits cleanly without creating any configuration" requirement.
- **Sync-first loop** — restructure `start_delta_sync()` to run one sync pass immediately, then sleep, so startup and re-auth both get fresh data without a full-interval delay.
- **DeltaSyncTimer with error callback** *(low priority)* — extend `DeltaSyncTimer` to accept an error callback for auth-degradation detection, reducing code duplication between the cache crate and the app crate.

## Capabilities

### New Capabilities

*(none)*

### Modified Capabilities

- `app-lifecycle`: Add clarification that signal handler registration occurs during setup, and that re-authentication triggers both immediate delta sync and pending-write flush.
- `tray-app`: Add clarification that tray menu is rebuilt dynamically after mount-state changes and that wizard cancellation during first-run exits the process.

## Impact

- **Code**: `crates/filesync-app/src/main.rs` (signal handler, delta sync loop, crash recovery call), `crates/filesync-app/src/commands.rs` (sign_in flush + sync), `crates/filesync-app/src/tray.rs` (dynamic menu rebuild), `crates/filesync-cache/src/sync.rs` (optional: error callback on DeltaSyncTimer).
- **Dependencies**: No new crate dependencies. `tokio::signal` is already available via `tokio` with `full` features.
- **Risk**: Low — all changes are additive to existing working code; no API surface changes; no config format changes.
- **Testing**: Existing integration tests continue to pass; new tests for signal handling are difficult to automate (manual verification), but delta-sync-first and crash-recovery-on-reauth paths can be tested.
