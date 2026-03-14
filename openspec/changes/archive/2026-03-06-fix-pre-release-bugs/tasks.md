## 1. Fix Clippy Warnings

- [x] 1.1 Combine nested `if let` in `crates/carminedesktop-auth/src/manager.rs:30-31` — merge `if let Some(ref token) = state.access_token { if let Some(expires_at) = state.expires_at {` into a single `if let` with `&&` (let-chains)
- [x] 1.2 Replace `nonminimal_bool` in `crates/carminedesktop-auth/tests/auth_integration.rs:68` — change `!(now + buffer < expires_soon)` to `now + buffer >= expires_soon`
- [x] 1.3 Replace `nonminimal_bool` in `crates/carminedesktop-auth/tests/auth_integration.rs:75` — change `!(now + buffer < expires_at_boundary)` to `now + buffer >= expires_at_boundary`
- [x] 1.4 Replace `nonminimal_bool` in `crates/carminedesktop-auth/tests/auth_integration.rs:89` — change `!(now + buffer < already_expired)` to `now + buffer >= already_expired`
- [x] 1.5 Run `cargo clippy --all-targets --all-features` and verify zero warnings

## 2. Token Storage Verify-After-Write

- [x] 2.1 Modify `store_tokens()` in `crates/carminedesktop-auth/src/storage.rs:18-28` — after `entry.set_password()` returns `Ok(())`, add `entry.get_password()` verification: if read-back fails or returns different data, log warning and fall through to encrypted file fallback instead of returning early
- [x] 2.2 Run `cargo test -p carminedesktop-auth` — verify `token_serialization_roundtrip` and `encrypted_file_storage_roundtrip` tests still pass

## 3. Cache Clear Command

- [x] 3.1 Add `clear()` method to `SqliteStore` in `crates/carminedesktop-cache/src/sqlite.rs` — execute `DELETE FROM items; DELETE FROM delta_tokens; DELETE FROM sync_state;` (do NOT clear `cache_entries` — DiskCache manages that table via its own connection)
- [x] 3.2 Add `clear()` method to `CacheManager` in `crates/carminedesktop-cache/src/manager.rs` — orchestrate: `self.memory.clear()`, `self.sqlite.clear()`, `self.disk.clear().await` (skip writeback — pending uploads must survive cache clear)
- [x] 3.3 Add `clear_cache` Tauri command in `crates/carminedesktop-app/src/commands.rs` — stop all mounts, call `cache.clear()`, restart mounts if authenticated, update tray menu
- [x] 3.4 Register `commands::clear_cache` in `generate_handler![]` macro in `crates/carminedesktop-app/src/main.rs:184-196`
- [x] 3.5 Wire `clearCache()` in `crates/carminedesktop-app/dist/settings.html:178` — invoke the `clear_cache` Tauri command, show success/error feedback to user

## 4. Application Icons

- [x] 4.1 ~~Create `crates/carminedesktop-app/icons/` directory~~ — done (user provided `icon.svg`)
- [x] 4.2 Convert `crates/carminedesktop-app/icons/icon.svg` to required formats — `32x32.png`, `128x128.png`, `icon.icns`, `icon.ico` — using `cargo tauri icon` or ImageMagick
- [x] 4.3 Verify `tauri.conf.json` icon paths resolve to the new files

## 5. Headless Mode

- [x] 5.1 Remove `#[cfg(feature = "desktop")]` gates from shared imports in `crates/carminedesktop-app/src/main.rs` — move `AuthManager`, `GraphClient`, `CacheManager`, `InodeTable`, `MountHandle`, `MountConfig`, `cache_dir`, `HashMap`, `Arc`, `Mutex`, `RwLock`, `CancellationToken`, `AtomicBool` imports out from behind the desktop feature gate so headless code can use them
- [x] 5.2 Move `parse_cache_size()` helper out from behind `#[cfg(feature = "desktop")]` gate (line 70-83) — headless needs it too
- [x] 5.3 Move `DEFAULT_CLIENT_ID` constant out from behind `#[cfg(feature = "desktop")]` gate (line 44) — headless needs it for AuthManager initialization
- [x] 5.4 Implement `run_headless()` component initialization in `crates/carminedesktop-app/src/main.rs` — create `AuthManager`, `GraphClient`, `CacheManager`, `InodeTable` using the same pattern as `run_desktop()` lines 132-163
- [x] 5.5 Implement headless authentication flow — `try_restore()` for existing tokens, fall back to `sign_in()` (browser-based PKCE via `open::that()`), exit with non-zero code if both fail
- [x] 5.6 Implement headless crash recovery — `cache.writeback.list_pending()` and re-upload pending writes (same logic as `run_crash_recovery()`)
- [x] 5.7 Implement headless mount startup — iterate enabled mounts from `EffectiveConfig`, call `MountHandle::mount()` directly, track drive IDs for sync
- [x] 5.8 Implement headless delta sync loop — `tokio::spawn` with `CancellationToken`, periodic `run_delta_sync()` per drive, auth degradation detection (log warning, keep running in degraded mode)
- [x] 5.9 Implement headless signal handling and graceful shutdown — wait for SIGTERM/SIGINT, cancel sync, flush pending writes (30s timeout), unmount all, exit(0)
- [x] 5.10 Log "carminedesktop headless mode running — N mount(s) active" after successful initialization

## 6. Verification

- [x] 6.1 Run `cargo fmt --all -- --check` — verify formatting
- [x] 6.2 Run `cargo clippy --all-targets --all-features` — verify zero warnings with `-Dwarnings`
- [x] 6.3 Run `cargo test --all-targets` — verify all tests pass
- [x] 6.4 Run `cargo build --all-targets` — verify clean build (headless + desktop)
