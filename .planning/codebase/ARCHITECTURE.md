# Architecture

**Analysis Date:** 2026-03-10

## Pattern Overview

**Overall:** Layered, multi-tier caching system with platform-abstracted VFS backends.

**Key Characteristics:**
- Multi-crate workspace (6 crates) separating concerns: auth, graph API, caching, VFS, and app orchestration
- Platform abstraction: single VFS core logic (`core_ops.rs`) used by both FUSE (Linux/macOS) and Cloud Files API (Windows)
- Async-first design with Tokio runtime, bridged to sync VFS trait methods via `block_on()`
- Delta sync polling for server-side change detection with observer pattern for kernel cache invalidation
- Multi-tier cache: memory (DashMap + TTL) → SQLite (metadata) → disk (content blobs) with write-back buffer

## Layers

**Authentication Layer:**
- Purpose: Manage OAuth2 PKCE flow and token lifecycle
- Location: `cloudmount-auth/src/`
- Contains: `AuthManager` (token state + refresh), OAuth2 flow, secure token storage (keyring → encrypted file fallback)
- Depends on: `cloudmount-core`, reqwest, keyring, crypto libraries (aes-gcm, argon2)
- Used by: `cloudmount-graph`, `cloudmount-app`

**Graph API Client:**
- Purpose: Wrapper around Microsoft Graph API v1.0 for file/folder operations
- Location: `cloudmount-graph/src/client.rs`
- Contains: HTTP client, delta query, item fetch, upload/download, copy operations with retry/backoff
- Depends on: `cloudmount-core`, `cloudmount-auth` (indirectly via token callback), reqwest
- Used by: `cloudmount-cache` (delta sync), `cloudmount-vfs` (file operations)

**Cache Layer:**
- Purpose: Multi-tier caching to reduce Graph API calls and optimize file access
- Location: `cloudmount-cache/src/`
- Contains:
  - `MemoryCache`: in-memory DashMap with TTL eviction
  - `SqliteStore`: persistent metadata index (parent-child relationships, etags)
  - `DiskCache`: blob storage with async eviction
  - `WriteBackBuffer`: staging area for pending uploads
  - `DeltaSyncTimer`: background polling loop for server changes
- Depends on: `cloudmount-core`, `cloudmount-graph`, rusqlite, tokio, dashmap
- Used by: `cloudmount-vfs`, `cloudmount-app`

**VFS Core Operations:**
- Purpose: Platform-independent filesystem logic (directory listing, file open/read/write, metadata)
- Location: `cloudmount-vfs/src/core_ops.rs`
- Contains: `CoreOps` struct with all file operations, cache lookups, conflict detection, write-back flushing
- Delegates to: Cache layer (lookup), Graph API (fetch/upload), inode table (mapping)
- Used by: FUSE backend (`fuse_fs.rs`), Cloud Files API backend (`cfapi.rs`)

**VFS Backends:**
- **FUSE (Linux/macOS):**
  - Location: `cloudmount-vfs/src/fuse_fs.rs`
  - Implements: `fuser::Filesystem` trait methods, delegates to `CoreOps`
  - Provides: `FuseDeltaObserver` for kernel cache invalidation via `inval_inode()`
  - Entry point: `cloudmount_vfs::mount::MountHandle::mount()`

- **Cloud Files API (Windows):**
  - Location: `cloudmount-vfs/src/cfapi.rs`
  - Implements: `cloud_filter::SyncFilter` trait methods
  - Converts placeholders to real files on access
  - Entry point: `cloudmount_vfs::cfapi::CfMountHandle`

**App Orchestration Layer:**
- Purpose: Lifecycle management, Tauri desktop integration, configuration
- Location: `cloudmount-app/src/main.rs`
- Contains:
  - Mount start/stop lifecycle
  - Delta sync timer management
  - Tauri command handlers (IPC bridge to frontend)
  - AppState singleton holding auth, graph, per-mount caches
- Depends on: All other crates, tauri, tokio
- Used by: Frontend (via Tauri IPC), system initialization

**Shared Types:**
- Purpose: Definitions shared across all layers
- Location: `cloudmount-core/src/`
- Contains: `DriveItem`, `Drive`, `Site`, `DeltaSyncObserver` trait, error types, config system
- Dependency boundary: All crates depend on core; core depends on nothing else in workspace

## Data Flow

**Mount Initialization:**

1. User configures mount in TOML config
2. `start_mount` in app creates `AuthManager`, `GraphClient`, `CacheManager`, `InodeTable`
3. Fetch root item from Graph API, insert into cache
4. Call `MountHandle::mount()` (FUSE) or `CfMountHandle::mount()` (Windows)
5. VFS receives filesystem calls, routes to `CoreOps`

**Read File:**

1. FUSE/CfApi calls `CoreOps::open_file()` → `CoreOps::read()`
2. `CoreOps::read()` → memory cache hit? → disk cache hit? → Graph API download
3. If file > 256 MB or download already in progress, use `StreamingBuffer` (async chunk fetch via watch channel)
4. Return bytes to caller

**Write File:**

1. FUSE/CfApi calls `CoreOps::write()`
2. `CoreOps::write()` → `WriteBackBuffer::buffer()` (stage to disk)
3. On file close/flush → `CoreOps::flush_inode()`
4. Conflict detection: compare cached eTag vs server eTag
5. If conflict: upload as `{name}.conflict.{timestamp}`
6. Upload to Graph API, update cache with new eTag
7. Invalidate parent directory cache

**Delta Sync:**

1. `DeltaSyncTimer` runs on interval (default 60s)
2. `run_delta_sync()` calls `graph.delta_query()` with delta token
3. For each item in response:
   - If deleted: capture path/name, remove from all caches, notify observer
   - If eTag changed: invalidate disk cache, mark inode dirty, notify observer
   - Otherwise: upsert to memory + SQLite cache, invalidate parent
4. Observer (`FuseDeltaObserver` or CfApi observer) marks open handles stale, invalidates kernel cache
5. Next file access detects stale handle, re-opens with fresh content

**State Management:**

Memory layout:
```
AppState (Tauri/App)
├── auth: Arc<AuthManager>
├── graph: Arc<GraphClient>
├── mount_caches: HashMap<drive_id, (CacheManager, InodeTable, Observer)>
└── mounts: HashMap<drive_id, MountHandle | CfMountHandle>

CacheManager
├── memory: MemoryCache (DashMap<inode, DriveItem> + TTL)
├── sqlite: SqliteStore (parent-child indexes, etags)
├── disk: DiskCache (blob storage on disk)
├── writeback: WriteBackBuffer (pending uploads)
└── dirty_inodes: DashSet<inode> (set by delta sync)

InodeTable
├── next_inode: AtomicU64
└── maps: RwLock<{ item_id ↔ inode }>
```

## Key Abstractions

**CoreOps:**
- Purpose: Single implementation of all VFS operations, used by both backends
- Examples: `cloudmount-vfs/src/core_ops.rs` (1700+ lines)
- Pattern: Holds `graph`, `cache`, `inodes`, `rt` handle; all operations take mutable `&self` to block on async operations
- Key methods: `open_file`, `read`, `write`, `flush_inode`, `mkdir`, `unlink`, `rename`

**InodeTable:**
- Purpose: Bidirectional mapping between Graph API item IDs and FUSE inode numbers
- Pattern: `HashMap<inode ↔ item_id>` behind `RwLock`, persisted to SQLite on mount
- Allocates new inodes sequentially; avoids collisions by starting after max persisted inode
- Thread-safe via RwLock; allows concurrent reads, exclusive writes

**DeltaSyncObserver Trait:**
- Purpose: Decouple cache layer (where delta sync runs) from VFS layer (where kernel cache lives)
- Location: `cloudmount-core/src/types.rs`
- Implemented by: `FuseDeltaObserver` (FUSE), `CfApiDeltaObserver` (Windows)
- Callback: `on_inode_content_changed(ino)` called when server eTag changes

**StreamingBuffer:**
- Purpose: In-memory download buffer for concurrent chunk fetching
- Pattern: BTreeMap<chunk_index, Vec<u8>>, watch channel for progress notification
- Used when: File < 256 MB and download already in progress; wait on watch channel until chunk available
- Prevents: Multiple Graph API range requests for same file, kernel blocking on slow network

## Entry Points

**Desktop (Tauri):**
- Location: `cloudmount-app/src/main.rs` (run_desktop)
- Triggers: User runs `cloudmount` executable or system auto-start
- Responsibilities:
  - Initialize Tauri window + system tray
  - Setup AppState singleton (auth, graph, mount_caches)
  - Start delta sync timer per mount
  - Register Tauri command handlers (sign_in, add_mount, remove_mount, etc.)

**Headless (Systemd/Launchd):**
- Location: `cloudmount-app/src/main.rs` (run_headless)
- Triggers: `cloudmount --headless`
- Responsibilities:
  - Load config from file
  - Start all enabled mounts
  - Run delta sync timers
  - Block on graceful shutdown signal (SIGTERM/Ctrl-C)

**FUSE Mount:**
- Location: `cloudmount-vfs/src/mount.rs` (MountHandle::mount)
- Triggers: `start_mount` command from app
- Responsibilities:
  - Create `CloudMountFs` (implements `fuser::Filesystem`)
  - Mount at path via `fuser`
  - Set notifier for kernel cache invalidation
  - Return `MountHandle` for lifecycle management

**Cloud Files API Mount (Windows):**
- Location: `cloudmount-vfs/src/cfapi.rs` (CfMountHandle::mount)
- Triggers: `start_mount` command from app
- Responsibilities:
  - Register directory as Cloud Files sync root
  - Populate placeholders for lazy-loading
  - Return `CfMountHandle`

## Error Handling

**Strategy:** Centralised error enum in `cloudmount-core::Error`, propagated via `Result<T>` type alias.

**Error Types:**
- `Error::Auth(String)` — token missing, refresh failed, PKCE cancelled
- `Error::Cache(String)` — SQLite failure, disk I/O error
- `Error::GraphApi { status, message }` — 4xx/5xx from Microsoft Graph, deserialization failure
- `Error::Network(String)` — reqwest timeout, DNS failure
- `Error::Filesystem(String)` — mount failed, stale mount cleanup failed
- `Error::Other(anyhow::Error)` — catch-all for external errors

**Propagation:**
- VFS backends reply with `Errno::ENOENT` (not found), `Errno::EIO` (I/O error) based on error type
- App layer logs all errors; critical errors show toast notifications or error dialog
- Test failures via `cloudmount_core::Result<()>` for `?` propagation

**Resilience:**
- Graph API calls use `with_retry()` — exponential backoff, max 3 retries
- Token expiry handled transparently: `AuthManager::access_token()` checks validity, refreshes on demand
- Stale mounts auto-detected on startup (`cleanup_stale_mount`) and cleaned up

## Cross-Cutting Concerns

**Logging:**
- Framework: `tracing` crate with `tracing-subscriber`
- Pattern: `tracing::info!`, `tracing::error!`, `tracing::debug!` with structured fields (e.g., `tracing::debug!(ino, "...")`)
- Configuration: `RUST_LOG` env var or `--log-level` CLI flag
- Sample: "delta sync for {drive_id}: {} upserts, {} deletes" logged at debug level

**Validation:**
- Config: `UserConfig::load_from_file()` validates TOML schema, expands mount paths
- File paths: `resolve_relative_path()` ensures items have `parentReference.path`, guards against missing names
- PKCE: `oauth.rs` generates code_challenge, verifies code_verifier on token exchange

**Authentication:**
- Managed by `AuthManager`: PKCE flow → token exchange → refresh token storage
- Token lifecycle: stored in keyring (with fallback encrypted file), auto-refresh on expiry (5-min buffer)
- Scope: `Files.ReadWrite.All Offline_access` (full Graph API access)

**Concurrency:**
- Runtime: Tokio threadpool with `rt.block_on()` for VFS sync → async bridging
- Locks:
  - `RwLock` for read-heavy state (AuthState, config, inode maps)
  - `Mutex` for write-heavy state (mount HashMap, active sign-in)
  - `DashMap`/`DashSet` for concurrent access (memory cache, dirty inodes)
- Watch channels: delta sync progress notification (`DownloadProgress` watch)

**Performance Optimization:**
- Cache tiers avoid repeated Graph API calls (default 60s metadata TTL)
- Streaming buffer prevents redundant range requests during sequential download
- Copy polling uses exponential backoff (500ms → 5s, max 10s)
- Delta sync batches upserts/deletes into single SQLite transaction

---

*Architecture analysis: 2026-03-10*
