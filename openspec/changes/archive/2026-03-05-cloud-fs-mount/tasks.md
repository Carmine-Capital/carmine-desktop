## 1. Project Scaffolding

- [x] 1.1 Initialize Rust workspace with `cargo init --name filesync` and create workspace Cargo.toml with member crates: `filesync-core`, `filesync-graph`, `filesync-vfs`, `filesync-cache`, `filesync-auth`, `filesync-app`
- [x] 1.2 Add shared dependencies to workspace Cargo.toml: `tokio` (async runtime), `reqwest` (HTTP), `serde`/`serde_json` (serialization), `anyhow`/`thiserror` (errors), `tracing` (logging), `rusqlite` (SQLite)
- [x] 1.3 Set up platform-specific FUSE dependencies: `fuser` for Linux/macOS in `filesync-vfs/Cargo.toml`, create conditional compilation targets for Windows WinFSP FFI
- [x] 1.4 Create `build/defaults.toml` template with all sections documented and commented (tenant, branding, defaults, mounts), and `config/` directory structure for user config
- [x] 1.5 Set up CI with GitHub Actions: build matrix (Linux, macOS, Windows), `cargo clippy`, `cargo test`, `cargo fmt --check`

## 2. Authentication (microsoft-auth)

- [x] 2.1 Create `filesync-auth` crate with `AuthManager` struct holding token state (access_token, refresh_token, expiry)
- [x] 2.2 Implement OAuth2 PKCE flow: generate code_verifier/code_challenge, build authorize URL with scopes (User.Read, Files.ReadWrite.All, Sites.Read.All, offline_access), open system browser via `open` crate. If packaged tenant_id exists, use tenant-specific endpoint (`/{tenant_id}/oauth2/v2.0/authorize`) and add `domain_hint` parameter. Resolve client_id from packaged defaults or fall back to built-in default.
- [x] 2.3 Implement localhost HTTP callback listener (using `tokio` + `hyper`) to capture authorization code from redirect, with 120s timeout
- [x] 2.4 Implement token exchange: POST to Microsoft token endpoint with authorization code + PKCE verifier, parse access_token + refresh_token + expires_in
- [x] 2.5 Implement silent token refresh: detect near-expiry (5min window), refresh using refresh_token, handle invalid_grant by switching to re-auth mode
- [x] 2.6 Implement secure token storage via `keyring` crate: store/retrieve/delete tokens from OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service)
- [x] 2.7 Implement encrypted file fallback for token storage when OS keychain is unavailable (AES-256 with password-derived key)
- [x] 2.8 Implement sign-out: revoke tokens, clear keychain entry, clear in-memory state
- [x] 2.9 Write integration tests for auth flow using mock OAuth2 server

## 3. Graph API Client (graph-client)

- [x] 3.1 Create `filesync-graph` crate with `GraphClient` struct wrapping `reqwest::Client` with base URL, auth token injection, and default headers
- [x] 3.2 Define core data types: `DriveItem`, `Drive`, `Site`, `Permission`, `UploadSession`, `DeltaResponse` as Rust structs with serde derive
- [x] 3.3 Implement drive operations: `get_my_drive()`, `list_children(drive_id, item_id)` with automatic pagination (@odata.nextLink following)
- [x] 3.4 Implement file download: `download_content(drive_id, item_id)` for small files, `download_range(drive_id, item_id, offset, length)` for ranged reads
- [x] 3.5 Implement file upload: `upload_small(drive_id, parent_id, name, content)` for <4MB, `create_upload_session()` + `upload_chunk()` for large files with 10MB chunks
- [x] 3.6 Implement folder creation: `create_folder(drive_id, parent_id, name)`
- [x] 3.7 Implement delete: `delete_item(drive_id, item_id)` with HTTP 204 handling
- [x] 3.8 Implement move/rename: `update_item(drive_id, item_id, new_name, new_parent_id)`
- [x] 3.9 Implement delta query: `delta_query(drive_id, delta_token)` returning changed items + new delta token, handling 410 Gone for expired tokens
- [x] 3.10 Implement SharePoint site operations: `search_sites(query)`, `get_followed_sites()`, `list_site_drives(site_id)`
- [x] 3.11 Implement rate limiting: detect HTTP 429, parse Retry-After header, sleep and retry; exponential backoff for 5xx errors (3 retries: 1s, 2s, 4s with jitter)
- [x] 3.12 Write unit tests for all Graph client methods using `wiremock` or `mockito` for HTTP mocking

## 4. Cache Layer (cache-layer)

- [x] 4.1 Create `filesync-cache` crate with `CacheManager` struct coordinating all three tiers
- [x] 4.2 Implement Tier 1 — in-memory metadata cache: `HashMap<u64, CachedMetadata>` with TTL tracking, LRU eviction at 10k entries, thread-safe via `DashMap` or `RwLock`
- [x] 4.3 Implement Tier 2 — SQLite metadata store: create schema (items, delta_tokens, sync_state tables), implement CRUD operations, use WAL mode for concurrent reads
- [x] 4.4 Implement Tier 2 — delta sync integration: apply delta query results as a single SQLite transaction (insert/update/delete), store and retrieve delta tokens per drive
- [x] 4.5 Implement Tier 3 — disk content cache: store files at `<cache_dir>/<drive_id>/<item_hash>`, track eTag for validation, read/write with async file I/O
- [x] 4.6 Implement cache size management: track total disk cache size in SQLite, LRU eviction when max size exceeded, expose `set_max_size()` for hot-reload
- [x] 4.7 Implement write-back buffer: write pending changes to `<cache_dir>/pending/<drive_id>/<item_id>`, track dirty files, flush queue on close/sync
- [x] 4.8 Implement crash recovery for write-back buffer: on startup, scan pending directory and re-queue unflushed uploads
- [x] 4.9 Implement periodic delta sync timer: configurable interval (default 60s), run delta query per mounted drive, update all cache tiers
- [x] 4.10 Write tests for cache hit/miss scenarios, eviction logic, TTL expiry, and crash recovery

## 5. Virtual Filesystem — Linux/macOS (virtual-filesystem)

- [x] 5.1 Create `filesync-vfs` crate with `FileSyncFs` struct implementing `fuser::Filesystem` trait
- [x] 5.2 Implement inode management: inode allocation table mapping inodes to Graph item IDs, root inode (1) mapped to drive root, `InodeTable` struct with bidirectional lookup
- [x] 5.3 Implement `lookup()`: resolve name within parent inode by querying cache, return `FileAttr` with TTL
- [x] 5.4 Implement `getattr()`: return file attributes (size, mtime, ctime, mode 0644/0755, uid/gid from process) from cache, falling back to API
- [x] 5.5 Implement `readdir()`: return directory entries from cache with proper offset handling for sequential enumeration
- [x] 5.6 Implement `open()` and `release()`: track open file handles, increment/decrement reference counts
- [x] 5.7 Implement `read()`: serve from disk cache if available, otherwise download from API; implement read-ahead for sequential access (16MB prefetch)
- [x] 5.8 Implement `write()`: write to local buffer, mark file as dirty in cache manager
- [x] 5.9 Implement `flush()` and `fsync()`: trigger upload of dirty files to Graph API, `fsync` blocks until upload completes
- [x] 5.10 Implement `create()`: create placeholder inode, write to buffer, upload on flush
- [x] 5.11 Implement `mkdir()`: create folder via Graph API immediately, add to inode table and cache
- [x] 5.12 Implement `unlink()` and `rmdir()`: delete via Graph API, remove from inode table and cache, return ENOTEMPTY for non-empty dirs
- [x] 5.13 Implement `rename()`: handle same-dir rename and cross-dir move via Graph API `PATCH`
- [x] 5.14 Implement write conflict detection: compare eTag on upload, create `.conflict.<timestamp>` copy on mismatch
- [x] 5.15 Implement mount/unmount lifecycle: `mount()` with fuser session, `unmount()` with flush + timeout, SIGTERM graceful shutdown handler
- [x] 5.16 Write integration tests with a local FUSE mount using temporary directories

## 6. Virtual Filesystem — Windows via Cloud Files API (virtual-filesystem)

- [x] 6.1 Extract shared core logic from `FileSyncFs` into a reusable layer: the FUSE implementation in `fuse_fs.rs` (861 lines) directly mixes platform-specific FUSE callbacks with core logic (cache lookups, Graph API calls, inode management, write-back). Extract the core logic so that both the FUSE backend and the new CfApi backend can call into `CacheManager`, `GraphClient`, and `InodeTable` without duplicating business logic. This is NOT a shared VFS trait (FUSE and CfApi have incompatible paradigms) — it's ensuring the existing helper methods and cache interactions are accessible from both backends.
- [x] 6.2 Add `cloud-filter` (MIT, v0.0.6) and `windows` crate (with `Win32_Storage_CloudFilters` and `Win32_Storage_FileSystem` features) as `#[cfg(windows)]` workspace dependencies. Gate the Windows CfApi module with `#[cfg(target_os = "windows")]` in `filesync-vfs/src/lib.rs`. Verify the crate compiles on Linux (Windows deps excluded) and passes `cargo-xwin check` for the Windows MSVC target.
- [x] 6.3 Implement sync root registration: use `cloud_filter::root::SyncRootIdBuilder` and `SyncRootInfo` to register FileSync as a cloud sync provider with Windows. The sync root path should be configurable (default: `%USERPROFILE%\FileSync\<mount-name>`). Handle registration on first run and unregistration on mount removal. Set `CF_HYDRATION_POLICY_PROGRESSIVE` (unblock app as soon as sufficient data arrives, continue download in background).
- [x] 6.4 Implement `SyncFilter` trait from `cloud-filter`: handle `CF_CALLBACK_TYPE_FETCH_PLACEHOLDERS` (populate directory with placeholder files via `CfCreatePlaceholders()`, sourcing metadata from `CacheManager` or `GraphClient` on cache miss) and `CF_CALLBACK_TYPE_FETCH_DATA` (hydrate file content on access via `CfExecute()`, streaming from disk cache or downloading from Graph API). Bridge async operations using `rt.block_on()`.
- [x] 6.5 Implement local change detection: use `ReadDirectoryChangesW` (or `ReadDirectoryChangesExW`) to watch the sync root for local file modifications, creations, deletions, and renames. On detected change, queue the operation for upload/sync via `GraphClient`. This replaces the FUSE `write()`/`unlink()`/`rename()` callbacks — CfApi writes go directly to NTFS without involving our process.
- [x] 6.6 Implement sync status management: use `CfSetInSyncState()` and `CfUpdatePlaceholder()` to maintain correct sync status icons in Explorer (cloud icon for placeholders, green checkmark for hydrated/synced, sync arrows for in-progress). Integrate with the existing delta sync logic from `filesync-cache` to detect remote changes and update local placeholders.
- [x] 6.7 Implement conflict detection for CfApi: before uploading a locally modified file, compare the local eTag with the remote eTag (same logic as the FUSE backend). On mismatch, save the local version as `<filename>.conflict.<timestamp>`, download the remote version, update the placeholder, and notify the user.
- [x] 6.8 Implement Windows-specific lifecycle: sync root connect/disconnect via `cloud_filter::root::Session`, graceful shutdown with pending upload flush + timeout, Ctrl-C signal handling. Handle dehydration requests from Windows Storage Sense (auto-free disk space).
- [x] 6.9 Update CI workflow: add `cargo build -p filesync-vfs --target x86_64-pc-windows-msvc` to the `windows-latest` job (CfApi is built into Windows, no driver install needed). Add integration tests that register a sync root, populate placeholders, hydrate a file, and verify content.
- [x] 6.10 Test CfApi integration on Windows (CI + local VM): register sync root, verify it appears in Explorer navigation pane, browse folders (placeholder population), open a file (hydration), edit and save (local change detection + upload), rename, delete, disconnect sync root. Verify status icons update correctly.

## 7. SharePoint Browser (sharepoint-browser)

- [x] 7.1 Implement site search in `filesync-graph`: `search_sites(query)` returning site display name, URL, and ID
- [x] 7.2 Implement followed sites listing: `get_followed_sites()` for default site suggestions
- [x] 7.3 Implement library listing: `list_site_drives(site_id)` filtered to `driveType == "documentLibrary"`
- [x] 7.4 Implement subfolder browsing for library: reuse `list_children()` to let users navigate into a library before selecting mount root
- [x] 7.5 Implement SharePoint mount configuration: validate mount point, save site_id + drive_id + site_name + library_name to config
- [x] 7.6 Wire SharePoint browser into the first-run wizard and "Add Mount" flow in the tray app UI

## 8. Tray Application (tray-app)

- [x] 8.1 Initialize Tauri v2 project within the workspace: `filesync-app` crate as the Tauri entry point, with Rust backend commands
- [x] 8.2 Implement system tray: icon registration, context menu with mount list, status indicators (synced/syncing/error), and menu actions (Open, Mount/Unmount, Settings, Sign Out, Quit)
- [x] 8.3 Build first-run wizard UI with two modes: (a) Full wizard for generic builds (Sign In → Select Source → SharePoint browser → Mount Point → Confirm), (b) Pre-configured wizard for packaged builds (branded welcome → Sign In → auto-mount all packaged drives → success). Detect mode by checking for packaged defaults at startup.
- [x] 8.4 Build settings window UI: tabs for General (auto-start, notifications, sync interval), Mounts (list, add, remove, configure), Account (email display, sign out), Advanced (cache dir, max size, TTL, debug logging, clear cache)
- [x] 8.5 Implement Tauri commands (Rust→JS bridge): `sign_in`, `sign_out`, `list_mounts`, `add_mount`, `remove_mount`, `toggle_mount`, `get_settings`, `save_settings`, `search_sites`, `list_drives`, `refresh_mount`
- [x] 8.6 Implement OS-native notifications via Tauri notification plugin: mount success, sync conflict, auth expiry, network offline
- [x] 8.7 Implement minimize-to-tray behavior: window close minimizes to tray, only Quit exits the process
- [x] 8.8 Wire the FUSE daemon lifecycle to Tauri: start mounts on app launch, stop mounts on Quit, expose mount status to the tray menu

## 9. Packaged Defaults & Build-Time Config (packaged-defaults)

- [x] 9.1 Create `build/defaults.toml` parser: define `PackagedDefaults` struct with `[tenant]`, `[branding]`, `[defaults]`, and `[[mounts]]` sections, using serde with `#[serde(default)]` for all optional fields
- [x] 9.2 Implement `build.rs` to embed `build/defaults.toml`: add `cargo:rerun-if-changed=build/defaults.toml`, use `include_str!` in source with graceful handling when the file is empty or contains only comments
- [x] 9.3 Implement `PackagedDefaults::load()` that parses the embedded TOML at startup, returning an empty defaults struct if no config was baked in (generic build)
- [x] 9.4 Implement `{home}` template variable expansion in mount_point fields: resolve to the current user's home directory at runtime per platform
- [x] 9.5 Implement branding resolution: `app_name()` function that returns `packaged.branding.app_name` or "FileSync" as fallback, used by tray, window titles, and notifications

## 10. Configuration Persistence (config-persistence)

- [x] 10.1 Implement `UserConfig` struct with serde deserialization from TOML: only user-modified general settings, `Vec<MountConfig>` (user-added mounts), `Vec<MountOverride>` (overrides for packaged mounts by ID), `Vec<String>` (dismissed_packaged_mounts), `Vec<AccountMetadata>`
- [x] 10.2 Implement `EffectiveConfig` builder: two-layer merge of `PackagedDefaults` + `UserConfig` — settings use `user ?? packaged ?? builtin`, mounts are unioned by ID with user values overriding packaged values per field
- [x] 10.3 Implement platform-aware config directory resolution: `dirs` crate for `~/.config/filesync` (Linux), `~/Library/Application Support/filesync` (macOS), `%APPDATA%\FileSync` (Windows)
- [x] 10.4 Implement user config file creation: on first run, create an empty (or minimal) user config file; do NOT seed it with packaged defaults (the packaged layer is always read from the binary)
- [x] 10.5 Implement config validation on load: check TOML parse, validate mount points, handle corrupted file (backup + reset)
- [x] 10.6 Implement auto-start management: create/remove systemd user service (Linux), LaunchAgent plist (macOS), registry Run key (Windows)
- [x] 10.7 Implement hot-reload: watch config changes from settings UI, apply mount point changes (unmount+remount), cache size changes (evict), and sync interval changes (reschedule timer) without restart
- [x] 10.8 Implement "Reset to Default" per setting: remove the user override for a key, causing the effective value to revert to the packaged default
- [x] 10.9 Implement "Restore default mounts": clear the dismissed_packaged_mounts list, causing all packaged mounts to reappear
- [x] 10.10 Write tests for two-layer merge: packaged-only, user-only, override precedence, mount union, dismissed mounts, update scenarios (new packaged mount added, packaged mount removed)

## 11. Integration and Testing

- [x] 11.1 End-to-end test: authenticate → mount OneDrive → list files → read a file → write a file → verify on remote → unmount (Linux)
- [x] 11.2 End-to-end test: authenticate → browse SharePoint sites → mount document library → list files → read → unmount (Linux)
- [x] 11.3 Test offline behavior: mount drive, disconnect network, verify cached files remain readable, reconnect, verify sync resumes
- [x] 11.4 Test write conflict: modify a file locally and remotely simultaneously, verify `.conflict` copy is created and user is notified
- [x] 11.5 Test cache eviction: fill cache beyond max size, verify LRU eviction, verify evicted files re-download on access
- [x] 11.6 Test crash recovery: kill process with pending writes, restart, verify pending uploads resume
- [x] 11.7 Cross-platform smoke test on macOS: mount, list, read, write, unmount
- [x] 11.8 Cross-platform smoke test on Windows: mount (CfApi), list, read, write, unmount
- [x] 11.9 Test pre-configured build: create a `build/defaults.toml` with tenant + mounts, build, run fresh install, verify wizard shows simplified flow, verify auto-mount after sign-in
- [x] 11.10 Test update scenario: install v1 with packaged defaults, modify user settings, install v2 with changed packaged defaults, verify user overrides preserved and new packaged values applied

## 12. Packaging and Distribution

- [x] 12.1 Set up Tauri bundling for Linux: generate `.deb` and `.AppImage` via `tauri build`
- [x] 12.2 Set up Tauri bundling for macOS: generate `.dmg` with code signing and notarization
- [x] 12.3 Set up Tauri bundling for Windows: generate `.msi` installer, bundle WinFSP dependency check/install prompt
- [x] 12.4 Create Azure AD app registration for FileSync with required Graph API permissions and document the setup for admin consent
- [x] 12.5 Create GitHub Actions workflow template for pre-configured builds: inputs for tenant_id, client_id, SharePoint site details; generates `build/defaults.toml` from inputs; runs `cargo tauri build`; uploads installers as artifacts. Builders can fork the repo and use this action without local Rust setup.
- [x] 12.6 Write builder documentation: how to create `build/defaults.toml`, how to register an Azure AD app for your tenant, how to build and distribute pre-configured installers
- [x] 12.7 Write user-facing README: installation instructions per platform, FUSE driver prerequisites, first-run walkthrough, FAQ
