# Codebase Structure

**Analysis Date:** 2026-03-18

## Directory Layout

```
CarmineDesktop/
├── Cargo.toml                       # Workspace manifest — all deps declared here
├── Cargo.lock                       # Lockfile (committed)
├── Makefile                         # Build/test/lint targets (run inside toolbox container)
├── AGENTS.md                        # AI agent instructions
├── README.md                        # Project documentation
├── package.json                     # Node.js deps for Tauri frontend tooling
├── scripts/
│   └── release.sh                   # Release automation
├── .github/
│   └── workflows/
│       ├── ci.yml                   # CI: fmt, clippy, build, test
│       ├── build-installer.yml      # Build platform installers
│       └── release.yml              # Release workflow
├── crates/
│   ├── carminedesktop-core/         # Shared types, errors, config
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs               # Module exports
│   │   │   ├── types.rs             # DriveItem, Drive, Site, DeltaResponse, etc.
│   │   │   ├── error.rs             # Error enum + Result type alias
│   │   │   ├── config.rs            # UserConfig, EffectiveConfig, MountConfig, autostart
│   │   │   └── open_online.rs       # Office URI scheme helpers
│   │   └── tests/
│   │       └── config_tests.rs
│   ├── carminedesktop-auth/         # OAuth2 PKCE + token storage
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs               # Module exports, re-exports AuthManager
│   │   │   ├── manager.rs           # AuthManager (token state, sign-in/out lifecycle)
│   │   │   ├── oauth.rs             # PKCE flow, code exchange, token refresh
│   │   │   └── storage.rs           # Keyring + AES-256-GCM encrypted file fallback
│   │   └── tests/
│   │       └── auth_integration.rs
│   ├── carminedesktop-graph/        # Microsoft Graph API v1.0 client
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs               # Module exports, re-exports GraphClient
│   │   │   ├── client.rs            # GraphClient (all API operations)
│   │   │   └── retry.rs             # Exponential backoff with jitter
│   │   └── tests/
│   │       └── graph_tests.rs
│   ├── carminedesktop-cache/        # Multi-tier caching system
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs               # Module exports
│   │   │   ├── manager.rs           # CacheManager (facade composing all tiers)
│   │   │   ├── memory.rs            # MemoryCache (DashMap + TTL + LRU eviction)
│   │   │   ├── sqlite.rs            # SqliteStore (metadata persistence, delta tokens)
│   │   │   ├── disk.rs              # DiskCache (content blobs, LRU eviction, eTag tracking)
│   │   │   ├── writeback.rs         # WriteBackBuffer (pending uploads, crash-safe)
│   │   │   ├── sync.rs              # DeltaSyncTimer, run_delta_sync, DeltaSyncResult
│   │   │   ├── offline.rs           # OfflineManager (folder pinning, recursive download)
│   │   │   └── pin_store.rs         # PinStore (SQLite-backed pin records with TTL)
│   │   └── tests/
│   │       ├── cache_tests.rs
│   │       └── test_offline.rs
│   ├── carminedesktop-vfs/          # Virtual filesystem (FUSE / WinFsp)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs               # Module exports, platform-gated re-exports
│   │   │   ├── core_ops.rs          # CoreOps (shared VFS logic for both backends)
│   │   │   ├── inode.rs             # InodeTable (bidirectional inode ↔ item_id mapping)
│   │   │   ├── fuse_fs.rs           # FUSE backend (Linux/macOS) — cfg-gated
│   │   │   ├── winfsp_fs.rs         # WinFsp backend (Windows) — cfg-gated
│   │   │   ├── mount.rs             # MountHandle, stale mount cleanup — cfg-gated
│   │   │   ├── sync_processor.rs    # Async upload queue (debounce, semaphore, dedup)
│   │   │   └── pending.rs           # Crash recovery, pending write retry
│   │   └── tests/
│   │       ├── fuse_integration.rs
│   │       ├── sync_processor_tests.rs
│   │       ├── open_file_table_tests.rs
│   │       ├── transient_file_tests.rs
│   │       ├── stale_mount_tests.rs
│   │       ├── pending_retry.rs
│   │       └── offline_vfs_tests.rs
│   └── carminedesktop-app/          # Tauri desktop app entry point
│       ├── Cargo.toml
│       ├── build.rs                 # Tauri build script
│       ├── tauri.conf.json          # Tauri config (Linux/macOS)
│       ├── tauri.windows.conf.json  # Tauri config (Windows overrides)
│       ├── src/
│       │   ├── main.rs              # CLI args, Tauri setup, mount lifecycle, AppState
│       │   ├── commands.rs          # #[tauri::command] IPC handlers
│       │   ├── tray.rs              # System tray setup and menu handling
│       │   ├── notify.rs            # Desktop notification helpers
│       │   ├── shell_integration.rs # File associations, context menus, Explorer nav pane
│       │   ├── update.rs            # Auto-updater (periodic check via tauri-plugin-updater)
│       │   └── ipc_server.rs        # Windows named-pipe IPC (cfg-gated)
│       ├── dist/                    # Frontend (Vanilla JS, no build step)
│       │   ├── wizard.html          # Setup wizard page
│       │   ├── wizard.js            # Wizard logic
│       │   ├── settings.html        # Settings page
│       │   ├── settings.js          # Settings logic
│       │   ├── styles.css           # Shared styles
│       │   ├── ui.js                # Shared UI utilities (showStatus, etc.)
│       │   └── fonts/               # Bundled fonts
│       ├── capabilities/            # Tauri v2 capability manifests
│       ├── icons/                   # App icons (all platforms)
│       ├── resources/               # Bundled resources
│       ├── gen/                     # Generated Tauri code (committed)
│       ├── windows/                 # Windows-specific build resources (MSI, installer)
│       └── tests/
│           └── integration_tests.rs
├── docs/                            # Developer documentation
├── .planning/                       # GSD planning documents
└── target/                          # Build artifacts (gitignored)
```

## Directory Purposes

**`crates/carminedesktop-core/src/`:**
- Purpose: Foundation layer — shared types, error handling, configuration
- Contains: Serde-annotated Graph API types, `thiserror` error enum, TOML config system, platform-specific autostart logic
- Key files: `types.rs` (all Graph API model types), `error.rs` (unified error enum), `config.rs` (UserConfig/EffectiveConfig/MountConfig + autostart + mount point expansion)

**`crates/carminedesktop-auth/src/`:**
- Purpose: Microsoft OAuth2 PKCE authentication and secure token storage
- Contains: Auth state machine, PKCE flow with local HTTP callback server, keyring + encrypted file token storage
- Key files: `manager.rs` (AuthManager — the public API), `oauth.rs` (PKCE + token exchange), `storage.rs` (keyring → AES-256-GCM fallback)

**`crates/carminedesktop-graph/src/`:**
- Purpose: Typed Microsoft Graph API v1.0 client
- Contains: All Graph API operations (drives, items, children, delta, upload, copy, sites), retry logic
- Key files: `client.rs` (GraphClient with ~30 API methods), `retry.rs` (exponential backoff, 3 retries, 1s base delay)

**`crates/carminedesktop-cache/src/`:**
- Purpose: Multi-tier caching, write-back buffer, delta sync engine, offline pinning
- Contains: Memory cache (DashMap), SQLite metadata store, disk content cache, crash-safe writeback, delta sync loop, offline folder pinning
- Key files: `manager.rs` (CacheManager facade), `memory.rs` (TTL + LRU), `sqlite.rs` (schema + CRUD), `disk.rs` (content + eviction), `sync.rs` (delta sync), `writeback.rs` (pending uploads)

**`crates/carminedesktop-vfs/src/`:**
- Purpose: Virtual filesystem exposing OneDrive/SharePoint as local mount points
- Contains: Shared VFS operations, inode management, platform-specific FUSE/WinFsp backends, async upload processor, crash recovery
- Key files: `core_ops.rs` (CoreOps — ~1500 lines of shared VFS logic), `inode.rs` (InodeTable), `fuse_fs.rs` (FUSE backend), `winfsp_fs.rs` (WinFsp backend), `sync_processor.rs` (async upload queue), `mount.rs` (FUSE mount lifecycle)

**`crates/carminedesktop-app/src/`:**
- Purpose: Application entry point and desktop shell
- Contains: CLI parsing, Tauri initialization, mount lifecycle orchestration, IPC commands, system tray, notifications, shell integration, auto-updater
- Key files: `main.rs` (~1800 lines — AppState, setup, mount start/stop, delta sync loop, signal handling), `commands.rs` (~1500 lines — all Tauri IPC handlers)

**`crates/carminedesktop-app/dist/`:**
- Purpose: Frontend UI (Vanilla JS, no build step)
- Contains: Two pages (wizard + settings), shared styles and utilities
- Key files: `wizard.html`/`wizard.js` (setup flow), `settings.html`/`settings.js` (configuration), `ui.js` (shared `showStatus()` + Tauri IPC wrappers)

## Key File Locations

**Entry Points:**
- `crates/carminedesktop-app/src/main.rs`: Binary entry point, CLI parsing, desktop/headless dispatch
- `crates/carminedesktop-app/dist/wizard.html`: First-run setup wizard UI
- `crates/carminedesktop-app/dist/settings.html`: Settings UI

**Configuration:**
- `Cargo.toml`: Workspace root — ALL dependency versions declared here
- `crates/carminedesktop-app/tauri.conf.json`: Tauri config (app name, window, CSP, bundle ID)
- `crates/carminedesktop-app/tauri.windows.conf.json`: Windows-specific Tauri overrides
- `crates/carminedesktop-core/src/config.rs`: Runtime config system (TOML parsing, defaults, validation)
- `.env.example`: Environment variable template (existence noted; contents not read)
- `Makefile`: Build/test targets for toolbox container

**Core Logic:**
- `crates/carminedesktop-vfs/src/core_ops.rs`: Shared VFS operations (the heart of the system)
- `crates/carminedesktop-cache/src/manager.rs`: Cache tier composition
- `crates/carminedesktop-cache/src/sync.rs`: Delta sync engine
- `crates/carminedesktop-graph/src/client.rs`: All Graph API operations
- `crates/carminedesktop-auth/src/manager.rs`: Auth state machine

**Testing:**
- `crates/carminedesktop-core/tests/config_tests.rs`: Config parsing/validation tests
- `crates/carminedesktop-auth/tests/auth_integration.rs`: Auth flow tests
- `crates/carminedesktop-graph/tests/graph_tests.rs`: Graph API tests (wiremock)
- `crates/carminedesktop-cache/tests/cache_tests.rs`: Cache tier tests
- `crates/carminedesktop-cache/tests/test_offline.rs`: Offline pinning tests
- `crates/carminedesktop-vfs/tests/`: VFS tests (sync processor, open file table, transient files, stale mounts, pending retry, FUSE integration, offline VFS)
- `crates/carminedesktop-app/tests/integration_tests.rs`: App-level integration tests

## Naming Conventions

**Files:**
- Source modules: `snake_case.rs` (e.g., `core_ops.rs`, `sync_processor.rs`, `pin_store.rs`)
- Test files: `<module>_tests.rs` or `test_<module>.rs` in `crates/<name>/tests/`
- Frontend: `<page>.html` + `<page>.js` pairs in `dist/`

**Directories:**
- Crates: `carminedesktop-<concern>` (hyphenated, e.g., `carminedesktop-core`, `carminedesktop-vfs`)
- Platform-specific: gated with `#[cfg]` rather than separate directories

**Rust Identifiers:**
- Structs/Enums: `PascalCase` (e.g., `DriveItem`, `CacheManager`, `VfsError`)
- Functions: `snake_case` (e.g., `run_delta_sync`, `flush_inode_async`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `ROOT_INODE`, `SMALL_FILE_LIMIT`, `GRAPH_BASE`)
- Type aliases: `PascalCase` (e.g., `OpenerFn`, `TokenFuture`, `EvictionFilter`)

## Where to Add New Code

**New Graph API operation:**
- Add method to `GraphClient` in `crates/carminedesktop-graph/src/client.rs`
- Use `self.get_json::<T>()` for GET requests (includes retry logic)
- Add response type to `crates/carminedesktop-core/src/types.rs` if new
- Add test in `crates/carminedesktop-graph/tests/graph_tests.rs` with wiremock

**New VFS operation (e.g., new filesystem callback):**
- Add core logic to `CoreOps` in `crates/carminedesktop-vfs/src/core_ops.rs`
- Map `VfsError` to platform errors in `fuse_fs.rs` and `winfsp_fs.rs`
- Add test in `crates/carminedesktop-vfs/tests/`

**New Tauri IPC command:**
- Add `#[tauri::command]` function in `crates/carminedesktop-app/src/commands.rs`
- Register in `invoke_handler!` macro in `crates/carminedesktop-app/src/main.rs:606`
- Call from frontend via `window.__TAURI__.core.invoke("command_name", {...})` in the appropriate `.js` file

**New frontend page:**
- Create `<page>.html` + `<page>.js` in `crates/carminedesktop-app/dist/`
- Use `ui.js` helpers (`showStatus()`, `invoke()`)
- Open via `tray::open_or_focus_window()` in Rust
- No inline event handlers (CSP `script-src 'self'` blocks `onclick="..."`)

**New cache tier operation:**
- Add method to the relevant tier: `crates/carminedesktop-cache/src/memory.rs`, `sqlite.rs`, or `disk.rs`
- Expose through `CacheManager` if needed (or access tier directly via `cache.memory`, `cache.sqlite`, `cache.disk`)
- Add test in `crates/carminedesktop-cache/tests/cache_tests.rs`

**New configuration setting:**
- Add `Option<T>` field to `UserGeneralSettings` in `crates/carminedesktop-core/src/config.rs`
- Add resolved field to `EffectiveConfig` with default in `EffectiveConfig::build()`
- Add to `reset_setting()` match arm
- Add to `diff_configs()` if live reconfiguration needed
- Add to `save_settings` command in `crates/carminedesktop-app/src/commands.rs`
- Add UI control in `crates/carminedesktop-app/dist/settings.js`

**New error variant:**
- Add variant to `Error` enum in `crates/carminedesktop-core/src/error.rs`
- Add `VfsError` mapping in `VfsError::from_core_error()` if VFS-relevant (in `crates/carminedesktop-vfs/src/core_ops.rs`)

**New platform-specific feature:**
- Gate with `#[cfg(target_os = "...")]` or `#[cfg(feature = "desktop")]`
- Add shell integration in `crates/carminedesktop-app/src/shell_integration.rs`
- Add notification helpers in `crates/carminedesktop-app/src/notify.rs`

## Special Directories

**`target/`:**
- Purpose: Cargo build artifacts
- Generated: Yes
- Committed: No (gitignored)

**`crates/carminedesktop-app/gen/`:**
- Purpose: Tauri-generated code (e.g., `tauri.conf.json` schema types)
- Generated: Yes (by `tauri-build`)
- Committed: Yes

**`crates/carminedesktop-app/dist/`:**
- Purpose: Frontend assets (served by Tauri webview)
- Generated: No (hand-written Vanilla JS, no build step)
- Committed: Yes

**`crates/carminedesktop-app/icons/`:**
- Purpose: Application icons for all platforms (PNG, ICO, ICNS)
- Generated: No (design assets)
- Committed: Yes

**`crates/carminedesktop-app/windows/`:**
- Purpose: Windows-specific installer resources (WiX templates, MSI configuration)
- Generated: No
- Committed: Yes

**`crates/carminedesktop-app/capabilities/`:**
- Purpose: Tauri v2 capability manifests (permissions for plugins)
- Generated: No
- Committed: Yes

**`node_modules/`:**
- Purpose: Node.js dependencies (for Tauri frontend tooling)
- Generated: Yes (by `npm install`)
- Committed: No (gitignored)

**`.planning/`:**
- Purpose: GSD planning and codebase analysis documents
- Generated: By AI tooling
- Committed: Yes

---

*Structure analysis: 2026-03-18*
