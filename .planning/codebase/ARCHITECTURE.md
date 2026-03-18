# Architecture

**Analysis Date:** 2026-03-18

## Pattern Overview

**Overall:** Layered workspace architecture with clear dependency flow (core → auth/graph → cache → vfs → app), platform abstraction via `CoreOps`, and a multi-tier caching strategy (memory → SQLite → disk).

**Key Characteristics:**
- Strict unidirectional dependency graph: `core` has no workspace deps; `app` depends on all crates
- Platform-specific code isolated behind `#[cfg]` gates in the VFS and app crates
- Async runtime (Tokio) throughout, bridged to sync via `rt.block_on()` at VFS filesystem trait boundaries
- Feature-gated desktop GUI: `#[cfg(feature = "desktop")]` controls Tauri integration; headless mode runs without it
- Observer pattern (`DeltaSyncObserver` trait in core) breaks circular dependency between cache and VFS layers

## Dependency Graph

```
carminedesktop-core          (zero workspace deps — shared types, config, errors)
    ↑           ↑
    |           |
carminedesktop-auth    carminedesktop-graph
    |                      ↑
    |                      |
    |               carminedesktop-cache  (depends on: core, graph)
    |                      ↑
    |                      |
    |               carminedesktop-vfs    (depends on: core, graph, cache)
    |                      ↑
    ↑──────────────────────|
                    carminedesktop-app    (depends on: ALL crates)
```

## Layers

**Core Layer (`carminedesktop-core`):**
- Purpose: Shared types, error definitions, configuration system, and cross-cutting traits
- Location: `crates/carminedesktop-core/src/`
- Contains: `DriveItem`, `Drive`, `Site`, `DeltaResponse`, `Error`/`Result`, `UserConfig`/`EffectiveConfig`, `DeltaSyncObserver` trait, `MountConfig`, autostart logic, Office URI scheme helpers
- Depends on: External crates only (serde, chrono, thiserror, uuid, dirs, toml)
- Used by: Every other crate in the workspace

**Auth Layer (`carminedesktop-auth`):**
- Purpose: OAuth2 PKCE authentication with Microsoft identity platform, token lifecycle management
- Location: `crates/carminedesktop-auth/src/`
- Contains: `AuthManager` (token state behind `RwLock`), `oauth` (PKCE flow, local HTTP callback server), `storage` (keyring → AES-256-GCM encrypted file fallback)
- Depends on: `carminedesktop-core`
- Used by: `carminedesktop-app`

**Graph Layer (`carminedesktop-graph`):**
- Purpose: Microsoft Graph API v1.0 client with retry/backoff
- Location: `crates/carminedesktop-graph/src/`
- Contains: `GraphClient` (all API operations), `retry::with_retry` (exponential backoff with jitter)
- Depends on: `carminedesktop-core`
- Used by: `carminedesktop-cache`, `carminedesktop-vfs`, `carminedesktop-app`

**Cache Layer (`carminedesktop-cache`):**
- Purpose: Multi-tier caching (memory → SQLite → disk), write-back buffer, delta sync, offline pinning
- Location: `crates/carminedesktop-cache/src/`
- Contains: `CacheManager` (facade), `MemoryCache` (DashMap with TTL/LRU), `SqliteStore` (metadata persistence), `DiskCache` (content blobs with LRU eviction), `WriteBackBuffer` (crash-safe pending writes), `DeltaSyncTimer`/`run_delta_sync`, `OfflineManager`, `PinStore`
- Depends on: `carminedesktop-core`, `carminedesktop-graph`
- Used by: `carminedesktop-vfs`, `carminedesktop-app`

**VFS Layer (`carminedesktop-vfs`):**
- Purpose: Virtual filesystem exposing OneDrive/SharePoint as local mount points
- Location: `crates/carminedesktop-vfs/src/`
- Contains: `CoreOps` (shared business logic), `InodeTable` (bidirectional inode ↔ item_id mapping), `CarmineDesktopFs` (FUSE backend), `winfsp_fs` (WinFsp backend), `MountHandle` (lifecycle), `SyncProcessor` (async upload queue), `pending` (crash recovery)
- Depends on: `carminedesktop-core`, `carminedesktop-graph`, `carminedesktop-cache`
- Used by: `carminedesktop-app`

**App Layer (`carminedesktop-app`):**
- Purpose: Application entry point, Tauri desktop shell, system tray, notifications, CLI, shell integration
- Location: `crates/carminedesktop-app/src/`
- Contains: `main.rs` (CLI parsing, Tauri setup, mount lifecycle), `commands.rs` (IPC handlers), `tray.rs` (system tray), `notify.rs` (desktop notifications), `shell_integration.rs` (file associations, context menus, Explorer nav pane), `update.rs` (auto-updater), `ipc_server.rs` (Windows named-pipe IPC)
- Depends on: All workspace crates
- Used by: End user (binary entry point)

## Data Flow

**File Read (VFS → User):**

1. FUSE/WinFsp callback receives `read(ino, offset, size)` → delegates to `CoreOps::read_handle()`
2. `read_handle` checks `OpenFileTable` for active handle
3. For `Complete` state: slice buffer directly; for `Streaming` state: block until range available or issue on-demand range request
4. If handle is stale (delta sync detected remote change) and not dirty: re-download via `read_content()`
5. `read_content()` cascade: writeback buffer → disk cache (with eTag/size freshness check) → Graph API download
6. Downloaded content cached in disk cache for future reads

**File Write (User → OneDrive):**

1. FUSE/WinFsp `write` callback → `CoreOps::write_handle()` modifies in-memory buffer, marks dirty
2. FUSE/WinFsp `flush` callback → `CoreOps::flush_handle()`
3. Dirty content written to `WriteBackBuffer` (crash-safe on-disk persistence)
4. Upload delegated to `SyncProcessor` (async, debounced, max 4 concurrent uploads)
5. `flush_inode_async()`: conflict detection (eTag comparison), conflict copy if needed, then `GraphClient::upload()` (small ≤4MB direct, large via upload session with 10MB chunks)
6. On success: writeback entry removed, disk cache updated, memory cache refreshed

**Delta Sync (Server → Local):**

1. `DeltaSyncTimer` fires at configurable interval (default 60s)
2. `run_delta_sync()` calls `GraphClient::delta_query()` with stored delta token
3. For each changed item: update SQLite + memory cache, invalidate disk cache if eTag changed
4. For deleted items: remove from all cache tiers, invalidate parent memory cache
5. If file has open handles with content change: mark handles stale via `DeltaSyncObserver::on_inode_content_changed()`
6. `DeltaSyncResult` returned with changed/deleted items for platform-specific processing (e.g., WinFsp placeholder updates)

**Authentication Flow:**

1. `AuthManager::sign_in()` → `oauth::run_pkce_flow()` → start local HTTP server, open browser
2. User authenticates at Microsoft → redirect to `localhost:{port}/callback` with auth code
3. `oauth::exchange_code()` → POST to token endpoint → `TokenResponse`
4. Tokens stored: try OS keyring first; on failure, encrypt (AES-256-GCM + Argon2id key derivation) and write to `{config_dir}/tokens_{account_id}.enc`
5. `finalize_sign_in()` sets account_id, migrates tokens from legacy client_id key

**State Management:**
- `AppState` (Tauri managed state): holds `AuthManager`, `GraphClient`, `UserConfig`/`EffectiveConfig` (both behind `Mutex`), mount handles, cache entries per drive, sync cancellation token
- Per-mount state stored in `mount_caches: HashMap<drive_id, (CacheManager, InodeTable, DeltaSyncObserver, OfflineManager, offline_flag)>`
- VFS inode state: `InodeTable` (bidirectional `HashMap<u64, String>` + `HashMap<String, u64>` behind `RwLock`)
- Open file state: `OpenFileTable` (DashMap of file handles → `OpenFile` with content buffer, dirty/stale flags)

## Key Abstractions

**CoreOps (`crates/carminedesktop-vfs/src/core_ops.rs`):**
- Purpose: Platform-agnostic VFS business logic — the single source of truth for cache lookups, Graph API interactions, conflict detection, and write-back operations
- Pattern: Both FUSE and WinFsp backends hold a `CoreOps` instance and delegate all business logic to it, keeping only platform-specific callback translation in the backend layer
- Key methods: `lookup_item()`, `find_child()`, `list_children()`, `read_content()`, `open_file()`, `write_handle()`, `flush_handle()`, `release_file()`, `truncate()`

**CacheManager (`crates/carminedesktop-cache/src/manager.rs`):**
- Purpose: Facade composing all cache tiers into a single coherent interface
- Contains: `MemoryCache` (DashMap, TTL-based), `SqliteStore` (metadata persistence), `DiskCache` (content blobs with LRU eviction), `WriteBackBuffer` (pending uploads), `PinStore` (offline pins)
- Pattern: All tiers created and wired in `CacheManager::new()`, including eviction protection filter linking `PinStore` → `DiskCache`

**InodeTable (`crates/carminedesktop-vfs/src/inode.rs`):**
- Purpose: Bidirectional mapping between kernel inodes (u64) and Graph API item IDs (String)
- Pattern: `allocate()` is idempotent — returns existing inode if item_id already mapped; `ROOT_INODE = 1` is reserved. Counter starts after `max_inode` from SQLite on resume.

**GraphClient (`crates/carminedesktop-graph/src/client.rs`):**
- Purpose: Typed Graph API v1.0 client with automatic pagination and error handling
- Pattern: Takes a `token_fn` closure (returns `Future<Output = Result<String>>`) — decouples from `AuthManager`. All GET requests go through `get_json<T>()` with `with_retry()` wrapper. Rate limiting (429) handled with `Retry-After` header. Upload dispatches to small (PUT) or large (upload session) based on 4MB threshold.

**EffectiveConfig (`crates/carminedesktop-core/src/config.rs`):**
- Purpose: Flattened, resolved configuration with defaults applied — no `Option` fields
- Pattern: Built from `UserConfig` via `EffectiveConfig::build()`. `diff_configs()` produces `ConfigChangeEvent` list for live reconfiguration.

**SyncProcessor (`crates/carminedesktop-vfs/src/sync_processor.rs`):**
- Purpose: Async upload queue with debouncing, concurrency limiting (semaphore), and deduplication
- Pattern: MPSC channel receives `SyncRequest::Flush`/`FlushSync`/`Shutdown`. Debounces duplicate flushes within 500ms window. Max 4 concurrent uploads via `Semaphore`. `FlushSync` variant blocks caller via `oneshot` channel until upload completes.

## Entry Points

**Desktop mode (`carminedesktop-app/src/main.rs` → `run_desktop()`):**
- Location: `crates/carminedesktop-app/src/main.rs:504`
- Triggers: Default when `feature = "desktop"` and `--headless` not passed
- Responsibilities: Initialize Tauri, register plugins (dialog, notification, updater, opener, deep-link, single-instance), set up system tray, restore auth tokens, mount all enabled drives, start delta sync loop, register signal handler for graceful shutdown

**Headless mode (`carminedesktop-app/src/main.rs` → `run_headless()`):**
- Location: `crates/carminedesktop-app/src/main.rs` (below line 1488)
- Triggers: `--headless` flag or when built without `desktop` feature
- Responsibilities: CLI-only operation — authenticate, mount via FUSE (Linux/macOS), block on shutdown signal

**Tauri IPC commands (`carminedesktop-app/src/commands.rs`):**
- Location: `crates/carminedesktop-app/src/commands.rs`
- Triggers: Frontend JS via `window.__TAURI__.core.invoke("command_name", {...})`
- Responsibilities: Bridge between Vanilla JS frontend and Rust backend — sign in/out, mount management, settings CRUD, site search, drive listing, file open online, offline pin management

**VFS mount entry (`carminedesktop-vfs/src/mount.rs` → `MountHandle::mount()`):**
- Location: `crates/carminedesktop-vfs/src/mount.rs:94`
- Triggers: `start_mount()` in app crate
- Responsibilities: Fetch/restore root item, pre-fetch root children, create `CarmineDesktopFs`, start FUSE background session, wire delta observer

## Error Handling

**Strategy:** Unified `thiserror` enum (`carminedesktop_core::Error`) with typed variants. Propagation via `carminedesktop_core::Result<T>`. `anyhow::Error` reserved for the `Other` variant only.

**Patterns:**
- VFS layer has its own `VfsError` enum (`NotFound`, `NotADirectory`, `DirectoryNotEmpty`, `PermissionDenied`, `TimedOut`, `QuotaExceeded`, `IoError`) with `from_core_error()` mapping
- FUSE backend maps `VfsError` → `libc::Errno` (ENOENT, EIO, EACCES, etc.)
- WinFsp backend maps `VfsError` → `NTSTATUS` codes
- Tauri commands map all errors to `String` via `.map_err(|e| e.to_string())`
- Graph API errors carry HTTP status code + message, with special handling for 412 (PreconditionFailed), 423 (Locked), 429 (rate limited with retry), 410 (delta token expired)
- Network errors trigger offline mode transition in VFS (`CoreOps::set_offline()`)

## Cross-Cutting Concerns

**Logging:**
- Framework: `tracing` + `tracing-subscriber` with `env-filter`
- Dual output: stderr (console) + rolling daily file (`{data_dir}/carminedesktop/logs/carminedesktop.log`)
- Configurable via CLI `--log-level`, env `CARMINEDESKTOP_LOG_LEVEL`, or `RUST_LOG` (in that priority order)
- Convention: `tracing::warn!` for recoverable errors, `tracing::error!` for unrecoverable, `tracing::debug!` for hot-path operations

**Validation:**
- Mount point validation: system directory rejection, duplicate detection, path normalization (in `crates/carminedesktop-core/src/config.rs:350`)
- Drive existence validation before mount: 404 removes from config, 403 skips, network error proceeds in offline mode (in `crates/carminedesktop-app/src/main.rs:1115`)
- Transient file detection: Office lock files (`~$*`), temp files (`~*.tmp`), system files (`Thumbs.db`, `.DS_Store`) excluded from upload (in `crates/carminedesktop-vfs/src/core_ops.rs:365`)

**Authentication:**
- Token refresh: proactive 5-minute buffer before expiry
- Storage: OS keyring primary → AES-256-GCM encrypted file fallback (with verify-after-write for keyring)
- Session state: `AuthState` behind `RwLock` in `AuthManager`; single `CancellationToken` ensures only one sign-in flow active

**Platform Abstraction:**
- VFS: `CoreOps` contains all shared logic; `fuse_fs.rs` and `winfsp_fs.rs` implement platform traits and delegate to `CoreOps`
- Autostart: systemd user unit (Linux), LaunchAgent plist (macOS), Registry Run key (Windows) — all in `crates/carminedesktop-core/src/config.rs:596`
- Shell integration: context menus, file associations, Explorer nav pane — all platform-gated in `crates/carminedesktop-app/src/shell_integration.rs`
- Case sensitivity: NTFS case-insensitive matching via `names_match()` helper, case-insensitive child lookup on Windows

**Offline Mode:**
- `AtomicBool` offline flag per mount, set on network errors, checked before Graph API calls
- Serves from cache tiers without freshness validation when offline
- Offline folder pinning: `PinStore` persists pin records with TTL, `OfflineManager` downloads folder contents recursively, `DiskCache` eviction skips pinned items via `is_protected()` parent-chain walk

---

*Architecture analysis: 2026-03-18*
