# Codebase Structure

**Analysis Date:** 2026-03-10

## Directory Layout

```
cloud-mount/
├── crates/
│   ├── cloudmount-core/         # Shared types, config system, error enum
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── types.rs         # DriveItem, Drive, Site, DeltaSyncObserver trait
│   │   │   ├── error.rs         # Error enum with thiserror
│   │   │   └── config.rs        # TOML parsing, mount config, path expansion
│   │   └── tests/
│   │
│   ├── cloudmount-auth/         # OAuth2 PKCE, token storage
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── manager.rs       # AuthManager — token state, refresh logic
│   │   │   ├── oauth.rs         # PKCE flow, token exchange
│   │   │   └── storage.rs       # Keyring + encrypted file fallback
│   │   └── tests/
│   │
│   ├── cloudmount-graph/        # Microsoft Graph API v1.0 client
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs        # GraphClient with get/list/upload/download/delta
│   │   │   └── retry.rs         # Exponential backoff retry wrapper
│   │   └── tests/
│   │
│   ├── cloudmount-cache/        # Multi-tier cache & delta sync
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── manager.rs       # CacheManager aggregating all tiers
│   │   │   ├── memory.rs        # MemoryCache with DashMap + TTL
│   │   │   ├── sqlite.rs        # SqliteStore — item index, delta token
│   │   │   ├── disk.rs          # DiskCache — blob storage with eviction
│   │   │   ├── writeback.rs     # WriteBackBuffer — pending uploads
│   │   │   └── sync.rs          # DeltaSyncTimer, run_delta_sync()
│   │   └── tests/
│   │
│   ├── cloudmount-vfs/          # Virtual filesystem (FUSE/CfApi)
│   │   ├── src/
│   │   │   ├── lib.rs           # Module re-exports (platform-gated)
│   │   │   ├── core_ops.rs      # CoreOps — shared VFS logic (1700+ lines)
│   │   │   ├── inode.rs         # InodeTable — item_id ↔ inode mapping
│   │   │   ├── pending.rs       # flush_pending() for recovery on mount
│   │   │   ├── fuse_fs.rs       # CloudMountFs impl Filesystem (Linux/macOS)
│   │   │   ├── cfapi.rs         # CfMountHandle impl SyncFilter (Windows)
│   │   │   └── mount.rs         # MountHandle lifecycle (FUSE only)
│   │   └── tests/
│   │
│   └── cloudmount-app/          # Desktop app & CLI
│       ├── src/
│       │   ├── main.rs          # Tauri/headless entry, mount lifecycle
│       │   ├── commands.rs      # Tauri command handlers (sign_in, add_mount, etc.)
│       │   ├── notify.rs        # Desktop notifications
│       │   ├── tray.rs          # System tray menu
│       │   └── update.rs        # Auto-update check
│       ├── dist/                # Vanilla JS frontend
│       │   ├── index.html
│       │   ├── ui.js            # Event listeners, API calls
│       │   ├── style.css
│       │   └── ...
│       ├── icons/               # App icons (ICO, PNG)
│       ├── build.rs             # Tauri build script
│       └── tests/
│
├── Cargo.toml                   # Workspace root, deps in [workspace.dependencies]
├── Cargo.lock
├── Makefile                     # Build targets (make build, make test, etc.)
├── config/                      # Configuration templates
├── docs/                        # Documentation
├── scripts/                     # Build/test scripts
├── openspec/                    # OpenSpec workflow (read-only specs)
├── .planning/codebase/          # GSD codebase analysis (this directory)
└── CLAUDE.md                    # Project conventions & anti-patterns
```

## Directory Purposes

**cloudmount-core:**
- Purpose: Shared types and configuration system — no external dependencies within workspace
- Contains: Error enum, DriveItem/Drive/Site structs, DeltaSyncObserver trait, TOML config parsing
- Key files:
  - `types.rs`: All struct definitions with `#[serde(rename = "camelCase")]` for Graph API JSON
  - `config.rs`: UserConfig, EffectiveConfig, path expansion (~/path → /home/user/path)
  - `error.rs`: One-line Error enum using `thiserror`

**cloudmount-auth:**
- Purpose: OAuth2 token management
- Contains: PKCE flow, token refresh, secure storage (keyring → AES-256-GCM encrypted fallback)
- Entry: `AuthManager::new()` creates instance; callers await `sign_in()`, `refresh()`, `sign_out()`
- Concurrency: `RwLock<AuthState>` for token state; `Mutex<CancellationToken>` for active PKCE flow

**cloudmount-graph:**
- Purpose: Microsoft Graph API wrapper with retry/backoff
- Contains: HTTP operations (GET/POST), delta query pagination, large file upload chunking
- Entry: `GraphClient::new(token_fn)` takes async closure for token refresh
- Patterns: All methods are `async`; `with_retry()` wraps calls with exponential backoff

**cloudmount-cache:**
- Purpose: Multi-tier caching to reduce Graph API load
- Tiers (in order of lookup):
  1. **Memory**: DashMap with TTL eviction per entry
  2. **SQLite**: Parent-child relationships, eTag tracking, delta token storage
  3. **Disk**: File content blobs with LRU eviction
  4. **Write-back**: Staging area for pending uploads before Graph API push
- Concurrency: Memory cache via `DashMap`, SQLite via `Mutex` (rusqlite is not async)
- Delta sync: `DeltaSyncTimer` spawns background task; `run_delta_sync()` polls Graph every 60s

**cloudmount-vfs:**
- Purpose: VFS abstraction + platform backends
- Core logic: `core_ops.rs` (1700+ lines) — all file operations, conflict detection, write-back
  - Used by both FUSE and CfApi; backends are thin adapters
  - Key type: `CoreOps { graph, cache, inodes, rt }`
- FUSE backend: `fuse_fs.rs` + `mount.rs`
  - Implements `fuser::Filesystem` trait (sync methods)
  - `CloudMountFs::new()` → `mount()` → `BackgroundSession`
  - `FuseDeltaObserver` implements `DeltaSyncObserver` for kernel cache invalidation
- CfApi backend: `cfapi.rs` (Windows only)
  - Implements `cloud_filter::SyncFilter` trait
  - Converts placeholders to real files on access
- Inode mapping: `inode.rs` — bidirectional map item_id ↔ inode number
  - Persisted to SQLite to avoid collisions on remount
  - Lock-free reads via `RwLock`; concurrent allocations serialized

**cloudmount-app:**
- Purpose: Tauri desktop app + headless CLI orchestration
- Mount lifecycle: `start_mount()` creates cache/inode/observer, calls VFS backend
  - Desktop: returns to Tauri command handler; tracks in `AppState::mounts`
  - Headless: blocks on shutdown signal, unmounts all on exit
- Delta sync: `DeltaSyncTimer::start()` spawns background task per drive
  - Driven from `cloudmount_cache::sync` module; observer pattern for kernel invalidation
- AppState: singleton holding auth, graph, per-mount caches
  - Accessed by Tauri commands via `app.state::<AppState>()`
- Frontend: vanilla JS in `dist/`, no build step, Tauri IPC via `window.__TAURI__.core.invoke()`

## Key File Locations

**Entry Points:**

| File | Purpose | Invoked By |
|------|---------|-----------|
| `cloudmount-app/src/main.rs` | CLI parsing, config loading, headless/desktop branching | System shell / systemd |
| `cloudmount-app/src/main.rs:run_desktop()` | Tauri window setup, command registration | Tauri app init |
| `cloudmount-app/src/main.rs:run_headless()` | Mount all drives, block on signals | Direct `cloudmount --headless` |
| `cloudmount-vfs/src/mount.rs:MountHandle::mount()` | FUSE mount lifecycle | `cloudmount-app` start_mount command |
| `cloudmount-vfs/src/cfapi.rs:CfMountHandle::mount()` | Cloud Files API registration | `cloudmount-app` start_mount command |

**Configuration:**

| File | Purpose | Format |
|------|---------|--------|
| `cloudmount-core/src/config.rs` | Config parsing, validation, mount setup | TOML |
| Default location | `~/.config/cloudmount/config.toml` (Linux), `~/Library/Preferences/cloudmount.toml` (macOS), `%APPDATA%\cloudmount\config.toml` (Windows) | — |
| Print template | `cloudmount --print-default-config` | TOML with comments |

**Core Logic:**

| File | Lines | Purpose |
|------|-------|---------|
| `cloudmount-vfs/src/core_ops.rs` | ~1700 | All VFS operations: lookup, open, read, write, mkdir, unlink, rename |
| `cloudmount-cache/src/sync.rs` | ~270 | Delta sync polling, cache updates, observer notifications |
| `cloudmount-auth/src/manager.rs` | ~250 | AuthManager — token refresh, PKCE orchestration |
| `cloudmount-graph/src/client.rs` | ~600 | Graph API client — requests, uploads, copy operations |
| `cloudmount-cache/src/disk.rs` | ~350 | DiskCache — blob storage, eviction tracking |

**Testing:**

| Directory | Type | Pattern |
|-----------|------|---------|
| `crates/cloudmount-*/tests/` | Integration tests | Async `#[tokio::test]`, return `cloudmount_core::Result<()>` |
| Test execution | Via cargo | `make test` runs all; `cargo test -p cloudmount-cache` runs one crate |
| HTTP mocking | `wiremock` | `MockServer::start().await`, register mocks with `respond_with()` |
| Time control | `tokio::time::pause()` | Pause clock for deterministic retry testing |
| File I/O | `std::env::temp_dir()` | Create temp files, cleanup before each test |

## Naming Conventions

**Files:**

- `*.rs`: Source code modules
- `lib.rs`: Crate root; re-exports public modules
- `main.rs`: Binary entry point (only in `cloudmount-app`)
- `*.test.rs` or `*/tests/`: Integration tests
- `build.rs`: Build script (Tauri in app crate)

**Directories:**

- `src/`: Source code (split by concern: auth, graph, cache, vfs, app)
- `tests/`: Integration tests (one file per feature area)
- `dist/`: Frontend assets (app crate only)
- `icons/`: App icons (app crate only)

**Rust Identifiers:**

- Types: `PascalCase` (e.g., `AuthManager`, `DriveItem`, `CoreOps`)
- Functions: `snake_case` (e.g., `run_delta_sync`, `resolve_relative_path`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `GRAPH_BASE`, `ROOT_INODE`, `COPY_POLL_MAX_MS`)
- Modules: `snake_case` (e.g., `cloudmount_core`, `cloudmount_auth`)
- Fields in serde structs: camelCase via `#[serde(rename = "camelCase")]` to match Graph API JSON

**Workspace Dependencies:**

- All dependencies in root `Cargo.toml` `[workspace.dependencies]`
- Crates reference via `{ workspace = true }` (no version duplication)

## Where to Add New Code

**New Feature (e.g., "sync OneDrive for Business"):**

1. **If it's a new Graph API operation:**
   - Add method to `GraphClient` in `cloudmount-graph/src/client.rs`
   - Follow existing pattern: `async fn operation() -> cloudmount_core::Result<T>`
   - Use `with_retry()` for resilience

2. **If it's cache-related (prefetch, eviction strategy):**
   - Add logic to `cloudmount-cache/src/manager.rs` or appropriate tier file
   - Update `CacheManager::new()` if adding new configuration
   - Add tests to `cloudmount-cache/tests/`

3. **If it's VFS-related (new filesystem operation):**
   - Add method to `CoreOps` in `cloudmount-vfs/src/core_ops.rs`
   - Implement in `CloudMountFs` (FUSE) and `SyncFilter` (CfApi)
   - Add tests to `cloudmount-vfs/tests/`

4. **If it's app-level (new Tauri command):**
   - Add `#[tauri::command]` function to `cloudmount-app/src/commands.rs`
   - Register in `invoke_handler!()` in `main.rs`
   - Call from frontend via `window.__TAURI__.core.invoke("cmd_name", {...})`

**New Component/Module:**

- Decide which crate it belongs to (core → auth → graph → cache → vfs → app layering)
- Create new file in `src/` (e.g., `src/my_module.rs`)
- Add `pub mod my_module;` to `lib.rs` or parent module
- Create `tests/my_module_tests.rs` if integration tests needed

**Utilities:**

- **Shared helpers (used by 2+ crates):** Add to `cloudmount-core/src/` as new module
- **Single-crate helpers:** Add as private module in respective crate
- **VFS helpers:** Add to `cloudmount-vfs/src/` (e.g., `pending.rs` for flush recovery)

## Special Directories

**Build Output:**

- `target/`: Cargo build artifacts (debug/, release/)
- Generated: Yes (created by `cargo build`)
- Committed: No

**Generated by Tauri:**

- `cloudmount-app/gen/`: Type definitions from Tauri commands
- `cloudmount-app/capabilities/`: Capability declarations (macOS/Windows)
- Generated: Yes (during `tauri build`)
- Committed: No (in `.gitignore`)

**Cache Directories at Runtime:**

- User cache: `~/.cache/cloudmount/` (Linux), `~/Library/Caches/cloudmount/` (macOS), `%LOCALAPPDATA%\cloudmount\` (Windows)
  - Contains: SQLite metadata DB, disk blob cache, write-back staging
  - Never committed

**Mount Points:**

- User-configured via TOML (default `~/Cloud/`)
- Created by user, not committed

**Configuration at Runtime:**

- User config: `~/.config/cloudmount/config.toml`
- Auth tokens: Keyring (OS-managed) or `~/.config/cloudmount/tokens_*.enc` (encrypted fallback)
- Never committed

---

*Structure analysis: 2026-03-10*
