## 1. Config: root_dir setting

- [x] 1.1 Add `root_dir: Option<String>` field to `UserGeneralSettings` in `filesync-core/src/config.rs`
- [x] 1.2 Add `root_dir: Option<String>` field to `DefaultSettings` in `filesync-core/src/config.rs`
- [x] 1.3 Resolve `root_dir` in `EffectiveConfig::build()` with fallback chain: user → packaged → "Cloud"
- [x] 1.4 Add `root_dir` field to `EffectiveConfig` struct
- [x] 1.5 Add helper function `derive_mount_point(root_dir, mount_type, site_name, lib_name) -> String` that generates `{home}/{root_dir}/OneDrive` or `{home}/{root_dir}/{SiteName}/{LibName}`
- [x] 1.6 Add `root_dir` to `save_settings` and `get_settings` commands in `commands.rs`

## 2. Auth: token restoration

- [x] 2.1 Add `pub async fn try_restore(&self, account_id: &str) -> Result<bool>` method to `AuthManager` in `filesync-auth/src/manager.rs`
- [x] 2.2 In `try_restore`: call `storage::load_tokens(account_id)`, populate `AuthState` if tokens found, return `true` if access token is valid or refresh succeeds
- [x] 2.3 Add `DEFAULT_CLIENT_ID` constant to `filesync-app/src/main.rs` for generic builds
- [x] 2.4 Resolve effective client_id as `packaged.client_id() || DEFAULT_CLIENT_ID` during initialization

## 3. AppState: expand with service managers

- [x] 3.1 Add `auth: Arc<AuthManager>` field to `AppState`
- [x] 3.2 Add `graph: Arc<GraphClient>` field to `AppState`
- [x] 3.3 Add `cache: Arc<CacheManager>` field to `AppState`
- [x] 3.4 Add `inodes: Arc<InodeTable>` field to `AppState`
- [x] 3.5 Add `mounts: Mutex<HashMap<String, MountHandle>>` field to `AppState`
- [x] 3.6 Add `sync_timer: Mutex<Option<DeltaSyncTimer>>` field to `AppState`
- [x] 3.7 Add `drive_ids: Arc<RwLock<Vec<String>>>` field to `AppState`
- [x] 3.8 Add `authenticated: AtomicBool` and `auth_degraded: AtomicBool` fields to `AppState`

## 4. Initialization: wire components in main.rs

- [x] 4.1 Create `AuthManager` with resolved client_id and tenant_id after config loading
- [x] 4.2 Create `GraphClient` with `Arc<AuthManager>` token provider closure
- [x] 4.3 Create `CacheManager` with cache_dir, db_path, max_bytes parsed from effective config, and metadata_ttl
- [x] 4.4 Add `parse_cache_size(size_str: &str) -> u64` helper to parse "5GB"/"500MB" strings to bytes
- [x] 4.5 Create `InodeTable` and `drive_ids` (Arc<RwLock<Vec<String>>>)
- [x] 4.6 Assemble full `AppState` struct with all managers and pass to `tauri::Builder::manage()`

## 5. Startup: token restore and mount start in Tauri setup

- [x] 5.1 In Tauri `.setup()` closure: read account from `effective.accounts`, attempt `auth.try_restore(account_id)`
- [x] 5.2 If restore succeeds: set `authenticated = true`, run crash recovery, start all enabled mounts, start delta sync timer
- [x] 5.3 If restore fails or no account exists: show wizard window (first_run detection already exists)
- [x] 5.4 Implement `start_mount(app: &AppHandle, mount_config: &MountConfig) -> Result<()>` helper — creates mountpoint dir, calls `MountHandle::mount()`, inserts into `mounts` HashMap, adds drive_id to `drive_ids`, sends `notify::mount_success`
- [x] 5.5 Implement `stop_mount(app: &AppHandle, mount_id: &str) -> Result<()>` helper — removes from `mounts` (unmount with flush), removes drive_id from `drive_ids`
- [x] 5.6 Implement `start_all_mounts(app: &AppHandle) -> Result<()>` — iterate enabled mounts from effective_config, call `start_mount` for each, skip and log failures
- [x] 5.7 Implement `stop_all_mounts(app: &AppHandle)` — iterate all active mounts, call `stop_mount` for each

## 6. Delta sync timer

- [x] 6.1 After starting mounts, create `DeltaSyncTimer::start(graph, cache, drive_ids, inode_allocator, sync_interval_secs)` and store in `AppState.sync_timer`
- [x] 6.2 Wire inode_allocator closure: `Arc::new(move |item_id: &str| inodes.allocate(item_id))`
- [x] 6.3 On sign-out or quit: call `sync_timer.lock().take().map(|mut t| t.stop())`

## 7. Commands: implement stubs

- [x] 7.1 Implement `sign_in` command: call `auth.sign_in().await`, on success call `graph.get_my_drive()` to discover OneDrive, save account metadata to user_config, auto-create OneDrive mount config with derived mount_point, call `start_all_mounts`, start delta sync timer, set `authenticated = true`
- [x] 7.2 Implement `sign_out` command: call `stop_all_mounts`, stop delta sync timer, call `auth.sign_out().await`, remove account from user_config, save config, set `authenticated = false`, show wizard window
- [x] 7.3 Implement `search_sites` command: call `graph.search_sites(&query).await`, map results to `Vec<SiteInfo>`
- [x] 7.4 Implement `list_drives` command: call `graph.list_site_drives(&site_id).await`, map results to `Vec<DriveInfo>`
- [x] 7.5 Implement `refresh_mount` command: trigger delta sync for the specific drive by calling `run_delta_sync` directly for the given drive_id

## 8. Commands: update mount management

- [x] 8.1 Update `add_mount` command: after saving config, call `start_mount` for the newly added mount if authenticated
- [x] 8.2 Update `remove_mount` command: call `stop_mount` before removing config, then save
- [x] 8.3 Update `toggle_mount` command: call `start_mount` or `stop_mount` based on new enabled state

## 9. Tray: wire menu actions

- [x] 9.1 Wire "Sign Out" menu event to call `sign_out` logic (stop mounts, clear auth, show wizard)
- [x] 9.2 Wire "Quit" menu event to perform graceful shutdown (stop sync timer, stop all mounts, exit)
- [x] 9.3 Update tray tooltip to reflect mount status: "{app_name} — {N} drives mounted" or "{app_name} — Re-authentication required"
- [x] 9.4 Add `update_tray_menu(app: &AppHandle)` helper that rebuilds tray menu items from current mount state

## 10. Auth degradation

- [x] 10.1 In delta sync error handler: detect `Error::Auth` containing "re-authentication required", set `auth_degraded = true`
- [x] 10.2 When `auth_degraded` becomes true: update tray tooltip, send `notify::auth_expired` notification
- [x] 10.3 In `sign_in` command success path: clear `auth_degraded = false`, trigger immediate delta sync, flush pending writes
- [x] 10.4 Add auth degradation detection wrapper around delta sync loop (catch auth errors, set flag, continue loop)

## 11. Crash recovery

- [x] 11.1 After mounts start and auth is confirmed: call `cache.writeback.list_pending().await`
- [x] 11.2 If pending writes found: log count, spawn background task to attempt upload for each
- [x] 11.3 In recovery task: for each (drive_id, item_id), read content from writeback, upload via graph, on success remove from writeback, on failure log and skip
- [x] 11.4 Use eTag conflict detection during recovery upload (already handled by graph.upload)

## 12. Graceful shutdown

- [x] 12.1 In Tauri `.setup()`: register signal handler for SIGTERM/SIGINT that performs ordered shutdown
- [x] 12.2 Implement `graceful_shutdown(app: &AppHandle)` — stop delta sync, stop all mounts, exit
- [x] 12.3 Wire tray "Quit" menu item to call `graceful_shutdown`
- [x] 12.4 Ensure `MountHandle::unmount()` flush timeout (30s) is respected during shutdown

## 13. First-run: generic flow (no packaged defaults)

- [x] 13.1 After sign-in success in wizard: call `graph.get_my_drive()` to discover OneDrive
- [x] 13.2 Prompt user for root directory name (default "Cloud"), check if `~/Cloud` exists, warn if so
- [x] 13.3 Create OneDrive mount config with `derive_mount_point(root_dir, "onedrive", None, None)`
- [x] 13.4 Save root_dir and mount config to user config, rebuild effective config
- [x] 13.5 Start OneDrive mount, show success screen "Your OneDrive is ready at ~/Cloud/OneDrive"

## 14. First-run: packaged flow (with defaults)

- [x] 14.1 After sign-in success in wizard: start all enabled packaged mounts via `start_all_mounts`
- [x] 14.2 Show success screen listing all mounted drives
- [x] 14.3 Send notification listing mounted drives, minimize to tray

## 15. Testing

- [x] 15.1 Add integration test for initialization sequence: config → auth → graph → cache → AppState assembly
- [x] 15.2 Add integration test for token restoration: mock stored tokens, verify mounts start without sign-in
- [x] 15.3 Add integration test for sign-in command: mock OAuth flow, verify OneDrive auto-discovery and mount creation
- [x] 15.4 Add integration test for sign-out command: verify mounts stopped, tokens cleared, config updated
- [x] 15.5 Add integration test for crash recovery: create pending writes, restart, verify re-upload attempted
- [x] 15.6 Add integration test for graceful shutdown: verify flush called, mounts unmounted, process exits cleanly
- [x] 15.7 Add integration test for auth degradation: mock token refresh failure, verify flag set and notification sent
