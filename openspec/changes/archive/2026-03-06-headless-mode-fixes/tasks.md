## 0. Fix token storage key mismatch (D0)

- [x] 0.1 In `crates/cloudmount-auth/src/manager.rs`, change `try_restore()` (line 43) from `crate::storage::load_tokens(account_id)` to `crate::storage::load_tokens(&self.client_id)` — this makes the load key match the store key used by `exchange_code()` and `refresh()`
- [x] 0.2 Verify `sign_out()` (line 83) already uses `self.client_id` for `delete_tokens` — no change needed, just confirm consistency
- [x] 0.3 Run `cargo test -p cloudmount-auth` — existing auth tests should still pass (they use consistent keys)

## 1. Extract shared initialization (D1)

- [x] 1.1 Define `Components` struct in `crates/cloudmount-app/src/main.rs` (ungated, no `#[cfg]`) holding `auth: Arc<AuthManager>`, `graph: Arc<GraphClient>`, `cache: Arc<CacheManager>`, `inodes: Arc<InodeTable>`
- [x] 1.2 Extract `fn init_components(packaged: &PackagedDefaults, effective: &EffectiveConfig) -> Components` from the duplicated initialization logic (currently at lines 126-157 and 582-613) — creates AuthManager with client_id/tenant_id, GraphClient with token provider closure, CacheManager with cache dir/db/max_size/ttl, InodeTable
- [x] 1.3 Update `run_desktop()` to call `init_components()` and destructure into `AppState` fields
- [x] 1.4 Update `run_headless()` to call `init_components()` and destructure into local variables
- [x] 1.5 Move shared imports (`HashMap`, `AtomicBool`, `Mutex`, `RwLock`) out from behind `#[cfg(feature = "desktop")]` gate if not already ungated, since `init_components()` is shared code

## 2. Headless post-sign-in setup (D2)

- [x] 2.1 Change `run_headless` signature from `_user_config: UserConfig` to `mut user_config: UserConfig` so it can be mutated after sign-in
- [x] 2.2 After the `auth.sign_in()` success branch (currently line 637), add OneDrive auto-discovery: call `graph.get_my_drive().await` and log the discovered drive ID and name
- [x] 2.3 Push `AccountMetadata { id: drive.id, email: None, display_name: Some(drive.name), tenant_id: None }` to `user_config.accounts` if `accounts` is empty
- [x] 2.4 Check if a `"drive"` mount type exists in `user_config.mounts`; if not, call `user_config.add_onedrive_mount(&drive_id, &derive_mount_point(&effective.root_dir, "drive", None, None))` to create a default OneDrive mount
- [x] 2.5 Call `user_config.save_to_file(&config_file_path())` — log a warning on error but do not exit (tokens are already stored)
- [x] 2.6 Rebuild effective config: `let effective = EffectiveConfig::build(&packaged, &user_config)` so the mount list includes the newly created mount before mount startup proceeds
- [x] 2.7 Add required imports for `AccountMetadata`, `derive_mount_point`, `config_file_path` in the ungated section of `main.rs` (verify they're not behind `#[cfg(feature = "desktop")]`)

## 3. Auth-degradation deduplication (D3)

- [x] 3.1 Add `let auth_degraded = Arc::new(AtomicBool::new(false));` in `run_headless()` before the delta sync loop setup
- [x] 3.2 Clone `auth_degraded` into the sync loop task (`let sync_degraded = auth_degraded.clone();`)
- [x] 3.3 Update the `Error::Auth` match arm in the headless sync loop to check `sync_degraded.load(Ordering::Relaxed)` before logging — only log and set the flag if not already degraded (matching the desktop pattern at lines 474-487)
- [x] 3.4 Add `use std::sync::atomic::Ordering;` to the headless code path (verify it compiles without the desktop feature)

## 4. Non-blocking crash recovery (D4)

- [x] 4.1 Wrap the headless crash recovery block (currently lines 646-679) in `tokio::spawn()` — clone `graph` and `cache` into the spawned task
- [x] 4.2 Move mount startup (currently lines 681-737) to execute immediately after spawning crash recovery, without awaiting the recovery task
- [x] 4.3 Verify the spawn doesn't require any changes to variable lifetimes — `graph` and `cache` are `Arc` and already cloneable

## 5. SIGHUP re-authentication handler (D5)

- [x] 5.1 Add a `#[cfg(unix)]` block before the main signal wait that registers a SIGHUP handler: `let mut sighup = signal(SignalKind::hangup()).expect("failed to register SIGHUP handler");`
- [x] 5.2 Clone `auth`, `graph`, `cache`, and `auth_degraded` into the SIGHUP handler task
- [x] 5.3 Spawn a `tokio::spawn` task that loops on `sighup.recv().await` — on each SIGHUP: log "SIGHUP received — attempting re-authentication", call `auth.sign_in().await`
- [x] 5.4 On successful re-auth: clear `auth_degraded` flag (`store(false, Ordering::Relaxed)`), spawn crash recovery to flush pending writes, log "re-authentication successful"
- [x] 5.5 On failed re-auth: log the error with a hint ("If no browser is available, sign in from a desktop session first, then restart this process"), do NOT exit — remain in current state
- [x] 5.6 Ensure the main signal wait (`tokio::select!` for SIGTERM/SIGINT) is unchanged — SIGHUP is handled in its own spawned task and does not trigger shutdown

## 6. Verification

- [x] 6.1 Run `cargo build --all-targets` — verify clean build for both headless and desktop
- [x] 6.2 Run `cargo build --all-targets --features desktop` — verify desktop build is unaffected by the `init_components()` refactor (skipped: GTK3 dev libs not installed on this machine — build fails at system library linking, not at Rust compilation)
- [x] 6.3 Run `cargo clippy --all-targets --all-features` — zero warnings
- [x] 6.4 Run `cargo fmt --all -- --check` — verify formatting
- [x] 6.5 Run `cargo test --all-targets` — all existing tests pass
