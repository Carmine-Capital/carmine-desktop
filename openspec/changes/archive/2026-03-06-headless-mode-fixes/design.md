## Context

Headless mode (`#[cfg(not(feature = "desktop"))]`) was implemented in the `fix-pre-release-bugs` change as a full lifecycle — auth, crash recovery, mount startup, delta sync, signal-based shutdown. A detailed audit against the app-lifecycle spec and the desktop code path uncovered 7 issues. The most critical is a token storage key mismatch in `carminedesktop-auth` that breaks token restoration in both modes. Two further bugs break headless first-time-use, two are behavioral defects, and two are structural improvements. Fixes span `crates/carminedesktop-auth/src/manager.rs` (1-line key fix) and `crates/carminedesktop-app/src/main.rs` (the rest).

**Token storage key mismatch** (affects both modes): In `AuthManager`, `exchange_code()` (manager.rs:114) and `refresh()` (manager.rs:138) store tokens via `store_tokens(&self.client_id, &tokens)` where `client_id` is the Azure AD app ID (e.g., `"00000000-0000-0000-0000-000000000000"`). But `try_restore(account_id)` (manager.rs:43) calls `load_tokens(account_id)` where `account_id` comes from `AccountMetadata.id` in the user config — which is set to `drive.id` (e.g., `"b!xYzAbCdEfGhIjK..."`) in commands.rs:70. These are completely different strings, so tokens are never found on restart. The `sign_out()` method (manager.rs:83) correctly uses `self.client_id` for deletion, consistent with store.

**Current desktop sign-in flow** (commands.rs:52-103) does 4 things headless does not:
1. Calls `graph.get_my_drive()` to discover the OneDrive drive
2. Writes `AccountMetadata { id, display_name }` to `user_config.accounts`
3. Creates a default OneDrive mount via `user_config.add_onedrive_mount()` if none exists
4. Saves `user_config` to disk with `save_to_file()`

Without steps 1-4, headless sign-in produces valid tokens with no persisted account ID (so `try_restore()` can never find them on next launch) and no mount configuration (so 0 mounts start).

## Goals / Non-Goals

**Goals:**
- Fix token restoration so tokens stored during sign-in can actually be found on restart (both modes)
- Make headless first-time sign-in produce a working, self-sustaining setup (account + mount + config persisted)
- Eliminate auth-degradation log spam (one warning, not N×M per cycle)
- Let mounts start immediately without waiting for crash recovery uploads
- Give headless operators a re-authentication path without restarting the process
- Reduce code duplication between `run_desktop()` and `run_headless()` initialization

**Non-Goals:**
- CLI argument parsing (e.g., `--account`, `--mount-point`) — separate change
- Daemon mode / background process / systemd integration — separate change
- Config hot-reload via file watcher — separate change
- Headless sign-out or cache-clear commands — separate change
- Windows CfApi headless support — explicitly deferred per spec

## Decisions

### D0: Fix token storage key in `try_restore()`

**Decision**: Change `try_restore()` in `crates/carminedesktop-auth/src/manager.rs` to use `self.client_id` as the storage lookup key instead of the caller-provided `account_id` parameter:

```rust
// Before (broken):
pub async fn try_restore(&self, account_id: &str) -> carminedesktop_core::Result<bool> {
    let tokens = match crate::storage::load_tokens(account_id)? {
//                                                  ^^^^^^^^^^  drive.id → NOT FOUND

// After (fixed):
pub async fn try_restore(&self, account_id: &str) -> carminedesktop_core::Result<bool> {
    let tokens = match crate::storage::load_tokens(&self.client_id)? {
//                                                  ^^^^^^^^^^^^^^  same key as store/refresh/delete
```

This makes all four storage operations consistent:

| Operation | Key used | Status |
|-----------|----------|--------|
| `exchange_code()` → `store_tokens` | `self.client_id` | Already correct |
| `refresh()` → `store_tokens` | `self.client_id` | Already correct |
| `sign_out()` → `delete_tokens` | `self.client_id` | Already correct |
| `try_restore()` → `load_tokens` | ~~`account_id`~~ → `self.client_id` | **Fixed** |

The `account_id` parameter is retained for logging purposes (to identify which account is being restored) but is no longer used as the storage key.

**Why this is correct for the current design**: carminedesktop supports a single authenticated user per instance. The `AuthManager` is constructed once with one `client_id`. Tokens for that session are stored under `client_id`. There is exactly one token set per application instance. The `account_id` (drive.id) is a user-domain concept that belongs in `AccountMetadata`, not as a storage key.

**Alternative considered**: Store tokens under `drive.id` (change store to match restore). Rejected — `drive.id` is not known at sign-in time (it's discovered AFTER sign-in via `graph.get_my_drive()`), so `exchange_code()` cannot use it. The `client_id` is available from construction.

**Alternative considered**: Store tokens under both keys. Rejected — wasteful duplication, confusing semantics, and both copies need cleanup on sign-out.

**Future multi-account note**: If carminedesktop ever supports multiple simultaneous accounts, token storage will need a redesign (e.g., per-account AuthManager instances, or a composite key like `{client_id}:{user_oid}`). That's a separate change.

### D1: Shared `init_components()` function

**Decision**: Extract a top-level function that both `run_desktop()` and `run_headless()` call:

```rust
struct Components {
    auth: Arc<AuthManager>,
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
}

fn init_components(packaged: &PackagedDefaults, effective: &EffectiveConfig) -> Components
```

This replaces the ~30 duplicated lines (main.rs:126-157 and 582-613). The function is synchronous (no async) — it creates the objects but doesn't start any lifecycle. `run_desktop()` unpacks the struct into `AppState`; `run_headless()` holds the `Arc<T>` values directly.

**Alternative considered**: A shared `RuntimeContext` trait that both modes implement. Rejected — over-engineering for two call sites. A simple struct + function is sufficient.

### D2: Post-sign-in setup in headless

**Decision**: After a successful `auth.sign_in()` in headless mode, mirror the desktop `commands::sign_in` logic:

1. Call `graph.get_my_drive()` to discover the OneDrive drive
2. Push `AccountMetadata { id: drive.id, display_name: Some(drive.name) }` to `user_config.accounts`
3. If no OneDrive mount exists in config, call `user_config.add_onedrive_mount(&drive_id, &mount_point)` with `derive_mount_point(&effective.root_dir, "drive", None, None)`
4. Call `user_config.save_to_file(&config_file_path())`
5. Rebuild `effective` via `EffectiveConfig::build(&packaged, &user_config)` so the mount list is current before mount startup

This requires `user_config` to be passed as a mutable value into `run_headless()` (dropping the `_` prefix). The `effective` config is rebuilt in-place after the config file is written.

**Key detail**: This code runs only on fresh sign-in (the `!authenticated` branch). On subsequent startups, tokens are restored via `try_restore()` and the config already has the account + mounts.

**Alternative considered**: Extract a shared `post_sign_in()` async function used by both desktop and headless. Rejected — the desktop version deeply depends on `AppState` and Tauri `AppHandle` (for tray updates, notifications, `start_all_mounts`). The headless version doesn't have those types. The logic overlap is only ~15 lines and diverges in the wiring, so inline duplication is acceptable.

### D3: Auth-degradation dedup via `AtomicBool`

**Decision**: Add a local `AtomicBool` flag (`auth_degraded`) in the headless sync loop, mirroring the desktop's `AppState.auth_degraded` pattern:

```rust
let auth_degraded = AtomicBool::new(false);

// In sync loop error handler:
if !auth_degraded.load(Ordering::Relaxed) {
    auth_degraded.store(true, Ordering::Relaxed);
    tracing::warn!("Re-authentication required — cached files remain accessible");
}
```

The flag is shared with the SIGHUP handler (via `Arc<AtomicBool>`) so it can be cleared on re-authentication.

**Alternative considered**: A counter that logs every Nth occurrence. Rejected — noisy even at reduced frequency, and the spec says "logs a warning" (singular).

### D4: Non-blocking crash recovery

**Decision**: Wrap the headless crash recovery block in `tokio::spawn()`, exactly as desktop does (main.rs:526). The recovery task runs in the background while mount startup proceeds immediately:

```rust
let recovery_graph = graph.clone();
let recovery_cache = cache.clone();
tokio::spawn(async move {
    // ... existing crash recovery loop ...
});
```

No other changes needed — the recovery logic doesn't interact with mount startup or the sync loop. Pending writes that are uploaded during recovery may overlap with the first delta sync pass, but this is safe because conflict detection in `flush_inode` compares eTags.

### D5: SIGHUP re-authentication handler (Unix only)

**Decision**: Register a `SIGHUP` handler alongside `SIGTERM`/`SIGINT`. On SIGHUP:

1. Log `"SIGHUP received — attempting re-authentication"`
2. Call `auth.sign_in().await` (opens browser via `open::that()`)
3. On success: clear the `auth_degraded` flag, run crash recovery (flush pending writes), log success
4. On failure: log error, remain in degraded mode (do not exit)

The handler is a separate `tokio::spawn` task that loops on `sighup.recv()` (can handle multiple SIGHUPs over the process lifetime). This requires the `auth`, `graph`, `cache`, and `auth_degraded` values to be shared via `Arc` with the handler task.

The signal wait block now uses `tokio::select!` across three branches: SIGINT (shutdown), SIGTERM (shutdown), and SIGHUP (re-auth, non-terminating — handled in its own spawned task, not in the main select).

**Implementation structure**:
```rust
// Spawn SIGHUP handler as background task (loops, doesn't terminate)
#[cfg(unix)]
{
    let mut sighup = signal(SignalKind::hangup()).expect("SIGHUP handler");
    let hup_auth = auth.clone();
    let hup_graph = graph.clone();
    let hup_cache = cache.clone();
    let hup_degraded = auth_degraded.clone();
    tokio::spawn(async move {
        loop {
            sighup.recv().await;
            // re-auth attempt ...
        }
    });
}

// Main signal wait — only SIGTERM/SIGINT trigger shutdown (unchanged)
```

**On Windows**: SIGHUP does not exist. No equivalent is added — Windows headless users must restart the process to re-authenticate. This is acceptable because Windows headless mode currently only logs a warning about CfApi not being supported anyway.

**Alternative considered**: Unix socket accepting `reauth` command. Rejected — too much infrastructure for this change. SIGHUP is the standard Unix convention for reload/re-init and requires zero new dependencies.

**Alternative considered**: Automatic re-auth attempt on every degradation detection. Rejected — this could spam browser windows if the refresh token is permanently revoked (e.g., admin action). SIGHUP is intentional operator action.

## Risks / Trade-offs

**[Risk] Existing tokens stored under old key after D0 fix** → Non-issue. Tokens were always stored under `client_id`, and after the fix, `try_restore` also looks under `client_id`. The fix makes load match store — no migration needed. Users who were re-authenticating on every restart will simply find their existing tokens on the next launch.

**[Risk] SIGHUP opens a browser on a headless server (no display)** → Mitigation: `open::that()` will fail and the error is logged. The process remains running in degraded mode. The log message should hint: "If no browser is available, sign in from a desktop session first, then restart this process." Same risk and mitigation as the existing first-run sign-in flow.

**[Risk] Post-sign-in config write could fail (permissions, disk full)** → Mitigation: Log the error but don't exit. Tokens are already stored in the keyring — the user can add the account manually to `config.toml`, or the next run will trigger sign-in again. This matches desktop behavior where `save_to_file` errors are propagated but don't crash the app.

**[Risk] Race between crash recovery and first delta sync** → No real risk. Both operations touch the writeback buffer but conflict detection is eTag-based. A file recovered by crash recovery and then seen by delta sync will either match (no-op) or conflict (`.conflict` copy created). This is the existing behavior — desktop already has this race.

**[Trade-off] Code duplication in post-sign-in** → Accepted. The desktop post-sign-in (commands.rs:52-103) and headless post-sign-in are ~15 lines of shared logic surrounded by mode-specific wiring (AppState vs. local variables, tray updates vs. logging). Extracting a shared function would require abstracting over `AppState` access patterns, which adds complexity without proportional benefit. If a third mode emerges, this should be revisited.
