## Context

carminedesktop's 6 crates are implemented and tested, but a pre-release audit revealed 5 issues blocking CI and packaging. The fixes span 3 crates (`carminedesktop-auth`, `carminedesktop-cache`, `carminedesktop-app`) plus build assets. The most architecturally interesting decisions are the token storage verification strategy and how headless mode reuses the desktop lifecycle without Tauri.

## Goals / Non-Goals

**Goals:**
- All tests pass (`cargo test --all-targets`)
- Zero clippy warnings (`cargo clippy --all-targets` and `--all-features`)
- `cargo tauri build --features desktop` succeeds (icons present)
- Token storage survives keyring backends that accept writes but don't persist
- Headless mode runs the full mount lifecycle (auth → mount → sync → shutdown)
- Cache clear button in settings UI is functional

**Non-Goals:**
- Headless-specific CLI arguments (e.g., `--account`, `--mount-point`) — future work
- Headless interactive sign-in prompt — requires pre-existing tokens from a prior desktop session
- Branded/custom icon design — placeholders are sufficient; final artwork is a design task
- Auto-update mechanism — separate change
- Windows CfApi headless testing — no Windows environment available

## Decisions

### D1: Verify-after-write for keyring token storage

**Problem**: `store_tokens()` calls `entry.set_password()` and if it returns `Ok(())`, immediately returns — trusting the keyring persisted the data. On systems where the Secret Service reports available but doesn't reliably persist (locked collection, null backend, session-scoped storage), the token is silently lost. The encrypted file fallback never runs because keyring didn't report an error.

**Decision**: After a successful `set_password()`, immediately call `entry.get_password()` to verify the data was actually stored. If the read-back fails or returns different data, warn and fall through to the encrypted file fallback.

```
store_tokens(account_id, tokens)
  │
  ├─ keyring::Entry::new() → Ok(entry)
  ├─ entry.set_password(serialized) → Ok(())
  ├─ entry.get_password() ← NEW VERIFY STEP
  │    ├─ Ok(data) && data == serialized → return Ok(())
  │    └─ Err(_) or mismatch → warn, fall through ↓
  │
  └─ store_tokens_encrypted(account_id, serialized)
```

**Alternatives considered**:
- *Always write both keyring AND encrypted file*: Rejected — the encrypted file is "less secure" per the spec; writing it unconditionally defeats the purpose of trying the keyring first.
- *Only use encrypted file*: Rejected — loses the security benefit of OS keychain on systems where it works properly.
- *Verify the encrypted file after write too*: Considered but not needed — `std::fs::write()` + `std::fs::read()` on the same path within the same process is reliable. The keyring has the unreliability because it's an IPC call to an external daemon.

### D2: Headless mode reuses desktop lifecycle via extracted shared module

**Problem**: The desktop code in `run_desktop()` and its helpers (`setup_after_launch`, `start_all_mounts`, `start_delta_sync`, `graceful_shutdown`) contains all the mount lifecycle logic but is tightly coupled to `tauri::AppHandle` for state access and `tauri::async_runtime::spawn` for task spawning.

**Decision**: Keep the code inline in `main.rs` rather than extracting a `runtime.rs` module. The headless `run_headless()` function will directly hold `Arc<T>` references to the same components (AuthManager, GraphClient, CacheManager, InodeTable) and implement its own lifecycle loop. This avoids a large refactor of the working desktop code path.

The headless flow:

```
run_headless(packaged, user_config, effective)
  │
  ├─ Build tokio multi-thread runtime
  ├─ Create AuthManager, GraphClient, CacheManager, InodeTable
  │   (same initialization as run_desktop lines 132-163)
  │
  ├─ try_restore(account_id)
  │    ├─ Ok(true)  → proceed to mount
  │    └─ Ok(false) → sign_in() (opens system browser for OAuth)
  │         ├─ Ok(()) → proceed to mount
  │         └─ Err(_) → log error, exit(1)
  │
  ├─ Crash recovery (flush pending writes)
  ├─ Start all enabled mounts (MountHandle::mount directly)
  ├─ Start delta sync loop (tokio::spawn with CancellationToken)
  │
  ├─ Wait for SIGTERM / Ctrl+C
  │    ├─ Cancel sync timer
  │    ├─ Unmount all drives (flush pending, 30s timeout)
  │    └─ exit(0)
  │
  └─ (blocks forever via std::future::pending until signal)
```

**Key difference from desktop**: Headless mode CAN attempt `sign_in()` if no tokens are found — the OAuth PKCE flow opens the system browser via `open::that()`, which works outside of Tauri. The user completes auth in their browser, the localhost callback receives the code, and headless proceeds. This means headless mode works for first-time setup too, not just pre-authenticated accounts.

**Alternatives considered**:
- *Extract shared `runtime.rs` module, refactor desktop to use it*: Rejected for this change — it's a clean design but touches every function in the working desktop path. Risk of regression is not justified when the goal is fixing bugs, not refactoring. Can be done as a follow-up.
- *Headless refuses to run without pre-existing tokens*: Rejected — the PKCE browser flow works without Tauri, so there's no reason to limit headless to pre-authenticated mode only.

### D3: Cache clear via new Tauri command + CacheManager method

**Problem**: The Advanced settings tab has a "Clear Cache" button that calls an empty JavaScript function. The tray-app spec requires this feature (spec line 121).

**Decision**: Add a `clear()` method to `CacheManager` that orchestrates clearing all tiers, then expose it as a `clear_cache` Tauri command.

**Cache clear sequence**:
1. Stop all active mounts (release file handles, stop FUSE sessions)
2. `cache.memory.clear()` — already exists, clears DashMap
3. `cache.sqlite.clear()` — **new method**: `DELETE FROM items; DELETE FROM delta_tokens; DELETE FROM sync_state;`
4. `cache.disk.clear()` — already exists, removes content dir + clears tracker table
5. Do **NOT** clear writeback buffer — pending uploads must survive cache clear
6. Restart mounts if authenticated

**Alternatives considered**:
- *Clear only disk cache, keep metadata*: Rejected — user expectation for "Clear Cache" is a full reset. Stale SQLite metadata with no disk content would cause errors.
- *Clear writeback too*: Rejected — pending writes represent unsaved user data. Clearing them would cause data loss.

### D4: Application icons

**Problem**: `tauri.conf.json` references 4 icon files in `icons/` that don't exist. The Tauri bundler requires them to build installers.

**Decision**: The user provided a source SVG icon at `crates/carminedesktop-app/icons/icon.svg`. Convert it to the required platform formats:
- `icons/32x32.png` — 32×32 PNG (tray icon, small contexts)
- `icons/128x128.png` — 128×128 PNG (app list, about dialogs)
- `icons/icon.icns` — macOS app icon (multi-resolution bundle)
- `icons/icon.ico` — Windows app icon (multi-resolution)

**Generation approach**: Use the `tauri icon` CLI command from the SVG (after rasterizing to PNG if needed), which generates all required formats automatically. Alternatively, convert with ImageMagick/`rsvg-convert`.

### D5: Clippy warning fixes (trivial)

**4 warnings, 2 locations**:

1. **`manager.rs:30`** — collapsible `if` statement:
   ```rust
   // Before: nested if
   if let Some(ref token) = state.access_token {
       if let Some(expires_at) = state.expires_at {
   // After: combined with &&
   if let Some(ref token) = state.access_token
       && let Some(expires_at) = state.expires_at {
   ```

2. **`auth_integration.rs:68,75,89`** — `nonminimal_bool`:
   ```rust
   // Before
   !(now + buffer < expires_soon)
   // After
   now + buffer >= expires_soon
   ```

No behavioral change. Pure style compliance for CI.

## Risks / Trade-offs

**[Risk] Verify-after-write adds latency to token storage** → Mitigation: One extra IPC round-trip to the keyring daemon. Measured in single-digit milliseconds. Only happens on sign-in and token refresh (rare events). Acceptable.

**[Risk] Headless sign-in requires a graphical environment for the browser** → Mitigation: On truly headless servers (no display), `open::that()` will fail and the error is propagated clearly. The user can sign in from a desktop session first, then run headless. Document this in the error message.

**[Risk] Cache clear while mounts are active could cause I/O errors** → Mitigation: Stop all mounts before clearing, restart after. Brief interruption for active file operations, but cache clear is a deliberate user action in settings.

**[Risk] Placeholder icons look unprofessional** → Mitigation: They're functional, not final. Can be replaced by dropping new files into `icons/` with no code changes. The priority is unblocking `cargo tauri build`.

**[Risk] Headless mode duplicates initialization code from desktop mode** → Mitigation: The shared logic is ~40 lines of component creation (AuthManager, GraphClient, CacheManager, InodeTable). Tolerable duplication for now. A `runtime.rs` extraction can be done as a follow-up refactor without behavioral changes.
