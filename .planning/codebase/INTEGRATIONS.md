# External Integrations

**Analysis Date:** 2026-03-10

## APIs & External Services

**Microsoft Graph API:**
- Primary integration: OneDrive and SharePoint document libraries
  - API: Microsoft Graph v1.0 (`https://graph.microsoft.com/v1.0`)
  - SDK/Client: `cloudmount-graph::GraphClient` in `crates/cloudmount-graph/src/client.rs`
  - Auth: Bearer token from OAuth2 flow
  - Key endpoints:
    - `/me/drive` - User's OneDrive
    - `/drives/{drive_id}` - Specific drive metadata
    - `/drives/{drive_id}/items` - Directory listings, file operations
    - `/drives/{drive_id}/items/{item_id}/content` - File download/upload
    - `/sites/{site_id}` - SharePoint site lookups
  - Retry strategy: `with_retry()` with exponential backoff
  - Rate limiting: Respects `Retry-After` header, returns `Error::GraphApi { status: 429, ... }`
  - Large file upload: Chunked upload (10MB chunks, see `UPLOAD_CHUNK_SIZE` in `client.rs`)
  - Conflict detection: Pre-upload eTag comparison for data loss prevention

**Microsoft Identity Platform (OAuth2):**
- Auth flow: OAuth2 PKCE (Proof Key for Code Exchange)
- Authority: `https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0`
- Client ID: `8ebe3ef7-f509-4146-8fef-c9b5d7c22252` (default in `cloudmount-app/src/main.rs`)
- Configurable tenant ID via `CLOUDMOUNT_TENANT_ID` env var
- Scopes: `User.Read Files.ReadWrite.All Sites.Read.All offline_access`
  - `User.Read` - Basic user identity
  - `Files.ReadWrite.All` - Full OneDrive/SharePoint file access
  - `Sites.Read.All` - Read SharePoint sites
  - `offline_access` - Refresh token for long-lived sessions
- Flow implemented in `crates/cloudmount-auth/src/oauth.rs`
- Callback: Local HTTP server on ephemeral port (127.0.0.1:N/callback)

## Data Storage

**Databases:**
- SQLite (bundled via rusqlite)
  - Connection: Via `rusqlite::Connection` in `crates/cloudmount-cache/src/sqlite.rs`
  - Client: `rusqlite` 0.32 with bundled SQLite
  - Purpose: Metadata cache (DriveItem records, change tracking, eTag history)
  - Location: `{cache_dir}/cloudmount/cache.db`
  - Configuration: WAL mode + NORMAL pragma (see CLAUDE.md)
  - Schema: Tracks file/folder metadata, delta sync tokens for change detection
  - Wrapped in `Mutex` (not async-safe)

**File Storage:**
- Local filesystem only
- Cache: `{cache_dir}/cloudmount/` directory
  - Defaults: Linux/macOS `~/.cache/cloudmount/`, Windows `%AppData%/cloudmount/`
  - Configurable via `CLOUDMOUNT_CONFIG` or UI settings (`config.toml`)
  - Max size: 5GB default, user-configurable
  - Content: Cached file blobs and metadata SQLite DB

**Caching:**
- Multi-tier architecture (memory → SQLite → disk):
  - **Memory**: DashMap with TTL (default 60s)
  - **SQLite**: Metadata cache (directories, file attributes)
  - **Disk**: Content blobs (full file caches)
  - Writeback buffer for pending uploads in memory
  - Cache eviction: LRU when exceeding `cache_max_size`
  - See `crates/cloudmount-cache/` for implementation

## Authentication & Identity

**Auth Provider:**
- Microsoft Identity Platform (Azure AD)
- Implementation: OAuth2 PKCE flow in `crates/cloudmount-auth/`

**Token Storage:**
- Primary: OS keychain (`keyring` crate, service name `cloudmount`)
- Fallback: AES-256-GCM encrypted file at `{config_dir}/cloudmount/tokens_{account_id}.enc`
  - Key derivation: Argon2id (64KB memory, 3 iterations)
  - Machine-specific password: `cloudmount-fallback-{USER}@{config_dir}`
  - Encryption format: `[16-byte salt][12-byte nonce][ciphertext]`
  - Zeroization: All key material cleared from memory
- Token path (encrypted): `{config_dir}/cloudmount/tokens_{account_id}.enc`
- Token lifecycle: Access token + refresh token, refresh on expiry

**Multi-account:**
- Support for multiple Microsoft 365 organizational accounts
- Account metadata stored in config: `accounts` list in `config.toml`
- Per-account token storage (keyed by account_id)

## Monitoring & Observability

**Error Tracking:**
- None (built-in). Structured errors via `cloudmount_core::Error` enum
- Error variants: Auth, GraphApi, Cache, Filesystem, Config, Network, PreconditionFailed, Locked, Io, Other
- All errors in Graph client responses logged via `tracing::warn!` and `tracing::error!`

**Logs:**
- Via `tracing` + `tracing-subscriber`
- Log level controllable via:
  - `CLOUDMOUNT_LOG_LEVEL` env var (default: info)
  - UI settings (`log_level` in config)
  - Subscriber filters: `env_filter` feature
- Formatted output: structured JSON or human-readable (configurable in Tauri app)
- Sensitive data NOT logged: token values redacted, only error messages logged

**Delta Sync Monitoring:**
- Change detection via `delta()` endpoint from Microsoft Graph
- Tracks eTag changes for content invalidation
- Observer trait: `DeltaSyncObserver` in `cloudmount-core` for cache invalidation
- FUSE-specific: Kernel cache invalidation via `inval_inode()` when remote changes detected

## CI/CD & Deployment

**Hosting:**
- Desktop app: Tauri bundles
  - Linux: AppImage
  - macOS: DMG
  - Windows: MSI
- No server-side hosting (local desktop app only)

**CI Pipeline:**
- GitHub Actions (referenced via `gh` CLI in .claude/commands)
- Cargo checks: `cargo fmt`, `cargo clippy`, `cargo test`
- CI enforces:
  - Zero warnings: `RUSTFLAGS=-Dwarnings`
  - All targets: `--all-targets --all-features`
  - Run via Makefile: `make check` (fmt-check + clippy + test)

**Build Process:**
- Container build: `cloudmount-build` toolbox (see Makefile)
- Commands:
  - `make build` - Compile all targets
  - `make build-desktop` - Add desktop feature
  - `make build-appimage` - Package Linux AppImage
  - `make test` - Run full test suite
  - `make clippy` - Lint with warnings-as-errors
  - `make fmt` / `make fmt-check` - Format check

**Auto-Updates:**
- Tauri plugin: `tauri-plugin-updater` 2
- Update mechanism: In-app check (UI-driven or scheduled)

## Environment Configuration

**Required env vars:**
- `CLOUDMOUNT_CLIENT_ID` - Microsoft App ID (defaults to built-in if unset)
- `CLOUDMOUNT_TENANT_ID` - Azure AD tenant ID (defaults to `common` multi-tenant)

**Optional env vars:**
- `CLOUDMOUNT_APP_NAME` - Display name (default: CloudMount)
- `CLOUDMOUNT_LOG_LEVEL` - Log level: debug, info, warn, error (default: info)
- `CLOUDMOUNT_CONFIG` - Custom config file path (defaults to platform standard)
- `.env` file support via `dotenvy` for development

**Secrets location:**
- OAuth tokens: OS keychain (primary) or encrypted file fallback
- Config file path: Platform-specific (see `dirs` crate)
  - Linux: `~/.config/cloudmount/config.toml`
  - macOS: `~/Library/Application Support/cloudmount/config.toml`
  - Windows: `%AppData%/cloudmount/config.toml`
- Cache dir: `{config_dir}/cloudmount/` (configurable)

## Webhooks & Callbacks

**Incoming:**
- OAuth callback server (local, temporary)
  - Endpoint: `http://localhost:{ephemeral_port}/callback`
  - Handles: Authorization code from Microsoft login redirect
  - Implementation: `tokio::net::TcpListener` in `oauth.rs`
  - Runs only during login flow (spun up, receives code, tears down)

**Outgoing:**
- Delta sync polling (not webhooks)
  - Uses Graph API `/delta()` endpoint for change detection
  - Polling interval: 60s default (configurable via `sync_interval_secs`)
  - No external webhooks sent

## Concurrency & Performance

**Concurrent Data Structures:**
- `DashMap` for memory cache (lock-free concurrent hashmap)
- `RwLock` for auth state (read lock for token check, write lock for refresh)
- `Mutex` for SQLite connection (rusqlite not Send, single-threaded access)
- `Mutex` for open file table (FUSE notifier and file handle tracking)

**Async Runtime:**
- Tokio with full features (multi-threaded scheduler)
- VFS trait methods are sync; bridges to async via `rt.block_on()`
- No deadlock risk between cache locks and `block_on` calls (documented anti-pattern)

---

*Integration audit: 2026-03-10*
