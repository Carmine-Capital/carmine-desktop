# External Integrations

**Analysis Date:** 2026-03-18

## APIs & External Services

**Microsoft Graph API v1.0:**
- Primary data source for all OneDrive and SharePoint operations
- Base URL: `https://graph.microsoft.com/v1.0` (hardcoded in `crates/carminedesktop-graph/src/client.rs`)
- SDK/Client: custom `GraphClient` struct in `crates/carminedesktop-graph/src/client.rs`
- HTTP client: `reqwest 0.12` with Bearer token auth
- Auth: OAuth2 access token injected via closure (`token_fn`)

**Endpoints used (all in `crates/carminedesktop-graph/src/client.rs`):**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/me/drive` | GET | Get current user's OneDrive |
| `/drives/{id}` | GET | Get specific drive / validate existence |
| `/drives/{id}/items/{id}/children` | GET | List folder contents |
| `/drives/{id}/root/children` | GET | List root folder contents |
| `/drives/{id}/items/{id}` | GET | Get single item metadata |
| `/drives/{id}/items/{id}/content` | GET | Download file content (full or range) |
| `/drives/{id}/items/{parent}:/{name}:/content` | PUT | Upload small file (< 4MB) |
| `/drives/{id}/items/{id}/createUploadSession` | POST | Create upload session for large files |
| `{uploadUrl}` | PUT | Upload chunks (10MB each) for large files |
| `/drives/{id}/items/{parent}/children` | POST | Create folder |
| `/drives/{id}/items/{id}` | DELETE | Delete item |
| `/drives/{id}/items/{id}` | PATCH | Rename/move item |
| `/drives/{id}/items/{id}/copy` | POST | Server-side copy |
| `{monitorUrl}` | GET | Poll copy operation status |
| `/drives/{id}/root/delta` | GET | Delta query for sync |
| `/sites?search={query}` | GET | Search SharePoint sites |
| `/me/followedSites` | GET | Get user's followed sites |
| `/sites/{id}/drives` | GET | List document libraries in a site |

**Rate Limiting:**
- Handles HTTP 429 with `Retry-After` header (in `crates/carminedesktop-graph/src/client.rs`)
- Retry logic in `crates/carminedesktop-graph/src/retry.rs`: 3 retries, exponential backoff (1s base) with jitter
- Retries on: 429 (rate limit), 5xx (server error), network errors

**Error Handling:**
- 412 Precondition Failed → `Error::PreconditionFailed` (eTag conflict)
- 423 Locked → `Error::Locked` (file checked out)
- 410 Gone on delta → full resync (expired delta token)
- Structured error parsing: `GraphErrorResponse` → `Error::GraphApi { status, message }`

**Microsoft Identity Platform (OAuth2):**
- Authorization endpoint: `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize`
- Token endpoint: `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token`
- Implementation: `crates/carminedesktop-auth/src/oauth.rs`
- Flow: OAuth2 Authorization Code with PKCE (S256)
- Client ID: `8ebe3ef7-f509-4146-8fef-c9b5d7c22252` (public client, no secret)
- Scopes: `User.Read Files.ReadWrite.All Sites.Read.All offline_access`
- Callback: local HTTP server on `127.0.0.1:{port}/callback` (port 0 = OS-assigned)
- Callback server: `hyper 1.8` HTTP1 server serving styled HTML response
- Timeout: 120 seconds for user to complete browser auth
- Cancellable via `CancellationToken`

**Tauri Updater (Auto-Update):**
- Endpoint: `https://static.carminecapital.com/carmine-desktop/latest.json`
- Config: `crates/carminedesktop-app/tauri.conf.json` → `plugins.updater`
- Implementation: `crates/carminedesktop-app/src/update.rs`
- Check interval: every 4 hours, 10-second startup delay
- Signed updates using minisign public key (configured in `tauri.conf.json`)
- Artifacts uploaded via rsync to `static.carminecapital.com` in release workflow

## Data Storage

**SQLite (Metadata Cache):**
- Library: `rusqlite 0.32` (bundled SQLite)
- Implementation: `crates/carminedesktop-cache/src/sqlite.rs`
- Database per drive: `{cache_dir}/drive-{safe_id}.db`
- Pragmas: `WAL` journal mode, `NORMAL` synchronous, 5000ms busy timeout
- Tables:
  - `items` — inode ↔ DriveItem mapping with parent_inode, etag, name, size, json_data
  - `delta_tokens` — per-drive delta sync tokens
  - `sync_state` — pending upload tracking
  - `pinned_folders` — offline pin expiry tracking
- Connection wrapped in `Mutex` (not `Send`-safe, no async)
- Batch deltas applied in a single transaction via `apply_delta()`

**SQLite (Disk Cache Tracker):**
- Separate tracker database for disk cache eviction
- Implementation: `crates/carminedesktop-cache/src/disk.rs`
- Table: `cache_entries` — tracks cached file content on disk
- Same pragmas as metadata cache

**In-Memory Cache (DashMap):**
- Implementation: `crates/carminedesktop-cache/src/memory.rs`
- Type: `DashMap<u64, CachedEntry>` — lock-free concurrent hash map
- TTL: configurable, default 60 seconds
- Max entries: 10,000 (evict to 8,000 on overflow, LRU)
- Stores: `DriveItem` metadata + optional children map per inode

**Disk Cache (File Content):**
- Implementation: `crates/carminedesktop-cache/src/disk.rs`
- Location: `{cache_dir}/content/` — flat files keyed by drive_id + item_id
- Max size: configurable, default 5GB
- Eviction: LRU, with protection for pinned/offline folders

**Write-Back Buffer:**
- Implementation: `crates/carminedesktop-cache/src/writeback.rs`
- Location: `{cache_dir}/` — temporary files for pending uploads
- Tracks dirty data before upload to Graph API

**File Storage:**
- Local filesystem only — no external object storage
- Cache directory: `{cache_dir}/carminedesktop/` (via `dirs::cache_dir()`)
- Config directory: `{config_dir}/carminedesktop/` (via `dirs::config_dir()`)
- Log directory: `{data_dir}/carminedesktop/logs/` (via `dirs::data_dir()`)

**Caching:**
- Multi-tier: Memory (DashMap, TTL) → SQLite (metadata) → Disk (content blobs)
- Delta sync: periodic polling of Graph API delta endpoint for incremental updates
- Default sync interval: 60 seconds

## Authentication & Identity

**Auth Provider:**
- Microsoft Identity Platform (Azure AD / Entra ID)
- Organizational Microsoft 365 accounts only (v1)
- Implementation: `crates/carminedesktop-auth/src/` (oauth.rs, manager.rs, storage.rs)

**Auth Flow:**
1. Local HTTP server starts on `127.0.0.1` (random port)
2. Browser opens authorization URL with PKCE challenge
3. User authenticates in browser → redirect to local callback
4. Code exchanged for access + refresh tokens
5. Tokens stored securely (see below)
6. Access token refreshed automatically (5-minute buffer before expiry)

**Token Management:**
- `AuthManager` in `crates/carminedesktop-auth/src/manager.rs`
- State: `RwLock<AuthState>` — read lock for token check, write lock for refresh
- Auto-refresh: triggered when access token expires within 5 minutes
- `invalid_grant` handling: specific error message prompting re-authentication

**Token Storage (primary — OS Keychain):**
- Library: `keyring 3.6`
- Service name: `carminedesktop`
- Key: account_id (migrated from client_id on first use)
- Verification: round-trip read after write to detect unreliable backends
- Implementation: `crates/carminedesktop-auth/src/storage.rs`

**Token Storage (fallback — Encrypted File):**
- Triggered when: keychain unavailable, write fails, or verification fails
- Encryption: AES-256-GCM
- Key derivation: Argon2id (64KB memory, 3 iterations, 32-byte output)
- Storage format: `[16-byte salt][12-byte nonce][ciphertext]`
- Machine password: `carminedesktop-fallback-{USER}@{config_dir}:{machine_id}`
- Machine ID sources: `/etc/machine-id` (Linux), `IOPlatformUUID` (macOS), `MachineGuid` registry (Windows)
- File location: `{config_dir}/carminedesktop/tokens_{account_id}.enc`
- Permissions: `0600` on Unix (owner read/write only)

## Monitoring & Observability

**Error Tracking:**
- None (no Sentry, Datadog, etc.)
- Errors logged via `tracing::error!` and `tracing::warn!`

**Logs:**
- Framework: `tracing` ecosystem (`tracing` + `tracing-subscriber` + `tracing-appender`)
- Output: dual — stderr (with ANSI) + daily rolling file (no ANSI)
- File location: `{data_dir}/carminedesktop/logs/carminedesktop.log`
- Filter: `RUST_LOG` env or `--log-level` CLI arg or `log_level` config
- Default level: `info`

**Desktop Notifications:**
- Library: `tauri-plugin-notification 2`
- Implementation: `crates/carminedesktop-app/src/notify.rs`
- Events notified: mount start/stop/failure, conflict detection, writeback failure, upload failure, file locked, offline pin status, FUSE unavailable, updates available

## CI/CD & Deployment

**Hosting:**
- Update server: `static.carminecapital.com` (private, rsync-based deployment)
- No cloud hosting — desktop application distributed as installers

**CI Pipeline:**
- GitHub Actions — `.github/workflows/ci.yml`
- 3-platform matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`
- Steps: checkout → Rust toolchain → system deps → cargo cache → fmt check → clippy (2x) → build → test
- `RUSTFLAGS=-Dwarnings` enforced globally

**Release Pipeline:**
- GitHub Actions — `.github/workflows/release.yml`
- Triggered by: tag push (`v*`)
- Version verification: tag must match `tauri.conf.json` version
- Build: `tauri-apps/tauri-action@v0` per platform
- Signing: `TAURI_SIGNING_PRIVATE_KEY` secret for update signatures
- Artifacts: `.AppImage` + `.deb` (Linux), `.app.tar.gz` + `.dmg` (macOS), `setup.exe` / `.nsis.zip` (Windows)
- Manifest: `latest.json` generated for Tauri updater
- Deployment: rsync to `static.carminecapital.com` via SSH deploy key

**Installer Build Pipeline:**
- GitHub Actions — `.github/workflows/build-installer.yml`
- Separate workflow for building installer artifacts

**Local Dev:**
- Toolbox container `carminedesktop-build` — cargo commands run inside container
- `make check` = fmt-check + clippy + test (matches CI)
- App runs on host (FUSE mounts invisible inside toolbox)

## Environment Configuration

**Required env vars (runtime):**
- None strictly required — defaults to hardcoded client ID and `common` tenant
- Optional: `CARMINEDESKTOP_CLIENT_ID`, `CARMINEDESKTOP_TENANT_ID`, `CARMINEDESKTOP_CONFIG`, `CARMINEDESKTOP_LOG_LEVEL`

**Required env vars (CI/Release):**
- `TAURI_SIGNING_PRIVATE_KEY` — minisign private key for update signing
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password for signing key
- `DEPLOY_SSH_KEY` — SSH key for rsync deployment to update server

**Secrets location:**
- GitHub Actions secrets (CI/CD)
- OS keychain or encrypted `.enc` files (runtime token storage)
- `.env` file present (contains `GH_TOKEN` for development tooling)

## Webhooks & Callbacks

**Incoming:**
- Local OAuth callback: `http://127.0.0.1:{port}/callback` — receives authorization code from Microsoft Identity Platform redirect
- Deep links: `carminedesktop://` protocol — handles `open-online` action with `?path=` parameter
- IPC server (Windows only): `crates/carminedesktop-app/src/ipc_server.rs` — receives commands from Explorer context menu
- Single-instance forwarding: second instance argv forwarded to first instance via `tauri-plugin-single-instance`

**Outgoing:**
- None — no webhook registrations with external services

## Platform-Specific Integrations

**Linux:**
- FUSE3: `fuser 0.17` — `crates/carminedesktop-vfs/src/fuse_fs.rs`
- Autostart: systemd user unit (`carminedesktop.service`)
- URL opening: `xdg-open` with cleaned `LD_LIBRARY_PATH`/`LD_PRELOAD`
- Display detection: `DISPLAY` / `WAYLAND_DISPLAY` env vars

**macOS:**
- macFUSE: `fuser 0.17` — same crate, detected via `/Library/Filesystems/macfuse.fs`
- Autostart: Launch Agent (`com.carmine-capital.desktop.agent.plist`)
- Activation policy: `Accessory` (no Dock icon, tray-only)
- Machine ID: `ioreg -rd1 -c IOPlatformExpertDevice` → `IOPlatformUUID`

**Windows:**
- WinFsp: `winfsp 0.12` — `crates/carminedesktop-vfs/src/winfsp_fs.rs`
- Autostart: Registry key `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- WinFsp detection: Registry `HKLM\SOFTWARE\WinFsp` (+ WOW6432Node fallback)
- PATH injection: WinFsp bin directory prepended to PATH at startup
- Explorer navigation pane: shell integration via `crates/carminedesktop-app/src/shell_integration.rs`
- File associations: `.docx`/`.xlsx`/`.pptx` → Carmine Desktop handler (context menu + default handler)
- Office URI schemes: `ms-word:ofe|u|`, `ms-excel:ofe|u|`, `ms-powerpoint:ofe|u|` for co-authoring
- Native error dialogs: `MessageBoxW` on release builds (no console)
- IPC server for context menu commands

---

*Integration audit: 2026-03-18*
