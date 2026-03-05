## Context

All six crates (core, auth, graph, cache, vfs, app) are individually implemented and tested. The app crate has a working Tauri shell — config loading, tray menu, notification helpers, and 11 registered commands — but no runtime wiring. Five commands are stubs (`sign_in`, `sign_out`, `search_sites`, `list_drives`, `refresh_mount`), `AppState` holds only config structs, and mounts defined in config are never started. The headless path creates a tokio runtime, logs "ready", and exits.

The crate APIs are well-defined:
- `AuthManager::new(client_id, tenant_id)` with `access_token()`, `sign_in()`, `sign_out()`
- `GraphClient::new(token_fn)` takes a closure returning a future that yields a token
- `CacheManager::new(cache_dir, db_path, max_bytes, ttl)` creates all four tiers
- `MountHandle::mount(graph, cache, inodes, drive_id, mountpoint, rt)` starts a FUSE/CfApi session
- `DeltaSyncTimer::start(graph, cache, drive_ids, inode_allocator, interval)` runs background sync

The challenge is wiring these together with correct ownership, initialization order, and lifecycle management inside Tauri's runtime model.

## Goals / Non-Goals

**Goals:**
- Wire all crate managers into `AppState` so the desktop app functions end-to-end
- Implement two first-run flows: auto-mount (packaged defaults) and guided (generic build)
- Make all stub commands functional — sign-in opens browser, search queries Graph API, etc.
- Add mount lifecycle: start on auth, stop on sign-out, restart on config change
- Add graceful shutdown: flush pending writes, stop sync, unmount all
- Add auth degradation: detect revoked tokens, keep cached reads alive, notify user
- Recover pending writes from previous crashes on startup

**Non-Goals:**
- Headless mode (separate change)
- Compile/packaging/installers (separate change)
- Changes to library crates (auth, graph, cache, vfs) — this is orchestration only
- Frontend webview implementation (wizard/settings HTML/JS) — backend commands only
- Multi-account support (v1 is single Microsoft 365 account)

## Decisions

### D1: AppState shape — flat struct with Arc-wrapped managers

**Decision**: Expand `AppState` with service managers as direct fields.

```
AppState {
    // Config (existing)
    packaged:          PackagedDefaults,
    user_config:       Mutex<UserConfig>,
    effective_config:  Mutex<EffectiveConfig>,

    // Services (new)
    auth:              Arc<AuthManager>,
    graph:             Arc<GraphClient>,
    cache:             Arc<CacheManager>,
    inodes:            Arc<InodeTable>,

    // Runtime state (new)
    mounts:            Mutex<HashMap<String, MountHandle>>,
    sync_timer:        Mutex<Option<DeltaSyncTimer>>,
    drive_ids:         Arc<RwLock<Vec<String>>>,
    authenticated:     AtomicBool,
    auth_degraded:     AtomicBool,
}
```

**Why**: Tauri's `.manage(state)` stores one instance accessible via `app.state::<AppState>()`. A flat struct keeps all state in one place. Arc wrapping is required because `GraphClient`, `CacheManager`, and `InodeTable` are shared across mount handles, delta sync, and Tauri commands.

**Alternative considered**: Separate `ServiceLayer` struct referenced from `AppState`. Rejected — adds indirection without benefit; Tauri state is already a single-type store.

### D2: Token provider closure — Arc\<AuthManager\> captured by GraphClient

**Decision**: Wrap `AuthManager` in `Arc` (not `Arc<RwLock>`) and capture in GraphClient's token closure.

```rust
let auth = Arc::new(AuthManager::new(client_id, tenant_id));
let auth_for_graph = auth.clone();
let graph = Arc::new(GraphClient::new(move || {
    let auth = auth_for_graph.clone();
    async move { auth.access_token().await }
}));
```

**Why**: `AuthManager` already uses `tokio::sync::RwLock` internally for its `AuthState`. The `access_token()` method takes `&self`, so `Arc<AuthManager>` is sufficient — no outer lock needed. `GraphClient::new` requires `Fn() -> Fut + Send + Sync + 'static`, which the closure satisfies.

**Alternative considered**: `Arc<RwLock<AuthManager>>`. Rejected — double-locking (outer RwLock + inner RwLock) is unnecessary and adds deadlock risk.

### D3: Token restoration on startup — add `try_restore` method to AuthManager

**Decision**: Add a `pub async fn try_restore(&self, account_id: &str) -> Result<bool>` method to `AuthManager` that calls `storage::load_tokens(account_id)` and pre-populates the internal `AuthState` if tokens are found.

**Why**: The existing `AuthManager` has no way to load persisted tokens. `sign_in()` always runs the full browser OAuth flow. On relaunch, the app needs to skip auth if valid tokens exist in the keyring. The `try_restore` method fits cleanly into `AuthManager`'s responsibility — it owns token lifecycle.

The app calls it during initialization:
1. Read account metadata from `UserConfig.accounts`
2. If account exists: call `auth.try_restore(&account.id).await`
3. If `true` (tokens loaded and not expired): proceed to mount
4. If `false`: show sign-in UI

**Note**: This is the one small addition to a library crate. It's a new public method on an existing struct, not a structural change.

### D4: Fallback client_id for generic builds

**Decision**: Define a `DEFAULT_CLIENT_ID` constant in `filesync-app` for generic (non-packaged) builds. The effective client_id is resolved as: `packaged.client_id() || DEFAULT_CLIENT_ID`.

**Why**: `AuthManager::new` requires a `client_id` for OAuth. Packaged builds set it via `build/defaults.toml`. Generic builds have all sections commented out, so `effective.client_id` is `None`. The project needs a registered Azure AD app with a known client_id for the generic/open-source distribution.

**Alternative considered**: Require users to register their own app and configure client_id. Rejected — violates "ultra simple first start" requirement.

### D5: Initialization sequence — Config → Auth → Graph → Cache → (restore → mount)

**Decision**: Initialize components in dependency order during `run_desktop()` setup, before Tauri's event loop starts.

```
1. Load config          (sync  — already done)
2. Create AuthManager   (sync  — AuthManager::new)
3. Create GraphClient   (sync  — GraphClient::new with token closure)
4. Create CacheManager  (sync  — CacheManager::new, creates dirs and SQLite)
5. Create InodeTable    (sync  — InodeTable::new)
6. Build AppState       (sync  — struct construction)
7. Start Tauri          (tauri::Builder...run)
8. In Tauri .setup():
   a. Setup tray
   b. Attempt token restore   (async — try_restore)
   c. If restored:
      - Run crash recovery     (async — flush pending writes)
      - Start all mounts       (sync via block_on — MountHandle::mount per drive)
      - Start delta sync timer (async — DeltaSyncTimer::start)
      - Update tray status
   d. If not restored:
      - Show wizard window
```

**Why**: Steps 1-6 are all synchronous constructors — no async needed before Tauri starts. The async work (token restore, mount start) happens in Tauri's `.setup()` closure, which has access to Tauri's runtime. This avoids creating a separate tokio runtime.

**Tauri runtime note**: Tauri v2 provides its own tokio runtime. Async commands run on it. The `.setup()` closure can spawn async work via `tauri::async_runtime::spawn`. The `Handle` for VFS `block_on()` calls is obtained from `tokio::runtime::Handle::current()` inside an async context.

### D6: Mount lifecycle — start/stop per config mount

**Decision**: Implement mount lifecycle as three operations on AppState:

- `start_mount(mount_config) -> Result<()>`: Create mountpoint dir, call `MountHandle::mount()`, add to `mounts` HashMap, add drive_id to `drive_ids` list, send mount_success notification.
- `stop_mount(mount_id) -> Result<()>`: Remove from `mounts` (triggers `MountHandle::unmount()` with 30s flush), remove drive_id from `drive_ids`.
- `restart_mount(mount_id)`: stop then start.

These are called from:
- Tauri `.setup()` — start all enabled mounts after token restore
- `sign_in` command — start all enabled mounts after successful auth
- `sign_out` command — stop all mounts
- `add_mount` command — start the newly added mount
- `remove_mount` command — stop the mount
- `toggle_mount` command — start or stop based on new enabled state
- Quit handler — stop all mounts (graceful shutdown)

**Why**: Centralizing mount lifecycle in a few methods avoids duplicating the start/stop logic across multiple command handlers.

### D7: Auth degradation — flag + notification, no VFS changes

**Decision**: When authentication fails (refresh token revoked), set `auth_degraded: AtomicBool` to `true`, update tray tooltip to "Re-authentication required", and send a notification. Do NOT change VFS behavior.

**Why**: The VFS already has natural graceful degradation:
- **Cached reads succeed**: CoreOps lookup chain goes memory → SQLite → Graph. If the item is in memory or SQLite cache, it returns without hitting Graph. Only uncached lookups fail with EIO.
- **Writes buffer locally**: `write()` goes to writeback buffer (local disk). The write itself succeeds. Only `flush()` (upload) fails.
- **Pending writes survive**: Failed flushes leave the writeback entry intact. It retries on next flush attempt and survives crashes.

The only app-level change needed is **detection and notification**:
1. Delta sync runs every 60s and calls `graph.delta_query()` which calls `auth.access_token()`
2. If this returns `Error::Auth` with "re-authentication required", the app catches it
3. Sets `auth_degraded = true`, updates tray, sends notification
4. When user re-authenticates: sets `auth_degraded = false`, triggers immediate delta sync, flushes pending writes

**Alternative considered**: Add `AuthDegraded` variant to `VfsError` and propagate through VFS. Rejected — the VFS layer doesn't need to know about auth state. It already returns EIO for any Graph failure, which is the correct POSIX behavior.

### D8: Crash recovery — proactive flush after mount start

**Decision**: After starting mounts, spawn a background task that calls `cache.writeback.list_pending()` and attempts to flush each pending write.

```
1. Call list_pending() — returns Vec<(drive_id, item_id)>
2. For each (drive_id, item_id):
   a. Read pending content
   b. Check eTag on server (conflict detection)
   c. Upload via graph.upload()
   d. On success: remove from writeback buffer
   e. On failure: log and skip (will retry on next delta sync cycle)
```

**Why**: Pending writes from a crashed session sit in `{cache_dir}/pending/{drive_id}/{item_id}`. Without proactive recovery, they'd only be flushed when a user opens and closes the file again. The background task ensures data reaches the server within one sync cycle of restart.

**Timing**: Run once after mounts start and auth is confirmed. Not on a loop — delta sync handles ongoing syncing.

### D9: Root directory — new `root_dir` config field

**Decision**: Add `root_dir: Option<String>` to `UserGeneralSettings` and `DefaultSettings`. Default value: `"Cloud"`. Mount points for auto-created mounts are derived as:

- OneDrive: `{home}/{root_dir}/OneDrive`
- SharePoint: `{home}/{root_dir}/{SiteName}/{LibName}`

The first-run flow prompts: "Where should your files appear?" with a text field pre-filled to `~/Cloud`. If `~/Cloud` already exists, show a warning and suggest an alternative.

**Why**: Users need a consistent, predictable location for all their mounted drives. A single root directory keeps the filesystem clean. The naming convention (`~/Cloud/OneDrive/`, `~/Cloud/Marketing Team/Documents/`) is self-explanatory.

**Config impact**: New field in `UserGeneralSettings.root_dir` and `DefaultSettings.root_dir` in `filesync-core/src/config.rs`. When resolving mount points for auto-created mounts, use: `effective.root_dir || "Cloud"`.

### D10: First-run flow routing

**Decision**: First run is detected by `!config_file_path().exists()` (already implemented). The flow branches on `packaged.has_packaged_config()`:

**Packaged flow** (org build):
1. Show wizard: branded welcome + "Sign in with Microsoft" button
2. On sign-in success: auto-mount all packaged mounts (drive_ids already in config)
3. Show success screen → minimize to tray

**Generic flow** (open-source build):
1. Show wizard: "Sign in with Microsoft" button (only UI element)
2. On sign-in success:
   a. Call `graph.get_my_drive()` to discover user's OneDrive drive_id
   b. Prompt for root directory name (default: "Cloud", check availability)
   c. Create `~/Cloud/OneDrive/` mount automatically
   d. Save mount config to user config file
3. Show success: "Your OneDrive is ready at ~/Cloud/OneDrive/" + "Add SharePoint libraries anytime from Settings" → minimize to tray

**Why**: Packaged builds already know what to mount (org pre-configured). Generic builds need OneDrive auto-discovery because users don't know their drive_id. SharePoint is skipped at first run to keep it simple — zero decisions beyond "sign in" and "pick a folder name".

### D11: Graceful shutdown — signal handling + ordered teardown

**Decision**: On quit (tray menu or app exit):

1. Stop delta sync timer (`sync_timer.stop()`)
2. For each mount in `mounts`:
   a. `MountHandle::unmount()` — flushes pending writes (30s timeout), unmounts FUSE/CfApi
3. Drop CacheManager (closes SQLite connections)
4. Exit process

In the Tauri `setup()`, register a window event handler for the "Quit" menu item that performs this sequence. The existing `mount.rs::shutdown_on_signal()` handles SIGTERM/SIGINT for the process level.

**Why**: Ordered teardown ensures pending writes reach the server before unmount, and unmount happens before process exit. The 30s timeout per mount is already implemented in `MountHandle::unmount()`.

## Risks / Trade-offs

**[Risk] AuthManager.try_restore is a library crate change** → Mitigation: It's a single new public method (`try_restore`) that calls existing `storage::load_tokens()`. No structural changes. Minimal blast radius.

**[Risk] No client_id for generic builds blocks sign-in** → Mitigation: Register a FileSync Azure AD app and embed the client_id as `DEFAULT_CLIENT_ID`. This must be done before the app can be used. Document in README.

**[Risk] Cached reads may serve stale data during auth degradation** → Mitigation: This is acceptable — the alternative (failing all reads) is worse. Data staleness is bounded by the last successful delta sync. Tray status clearly indicates degraded state.

**[Risk] Crash recovery flush may conflict with concurrent edits** → Mitigation: The flush uses eTag conflict detection (already implemented in `CoreOps::flush_inode`). If the server version changed, a `.conflict` copy is created.

**[Risk] First-run root directory collision** → Mitigation: Check if `~/Cloud` exists before suggesting it. If it does, append a suffix or let the user pick a different name. The check is a simple `Path::exists()` call.

**[Risk] Tauri runtime Handle availability in .setup()** → Mitigation: Tauri's `.setup()` runs within the async runtime. `tokio::runtime::Handle::current()` is available. All mount creation happens here or in async commands, so the Handle is always accessible.

**[Risk] MountHandle is not Send** → The `fuser::BackgroundSession` inside MountHandle may not be Send on all platforms. Mitigation: Store mounts in `Mutex<HashMap>` (not across await points). Mount creation and teardown happen within `block_on()` or synchronous contexts.
