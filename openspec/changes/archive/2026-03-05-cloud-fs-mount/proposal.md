## Why

Accessing OneDrive and SharePoint files from the desktop requires either a web browser or the official Microsoft client, which is unavailable on Linux, limited to OneDrive personal/business (no SharePoint site-level mounting), and provides no lightweight read/write virtual-drive experience. Professionals using Microsoft 365 in mixed-OS environments need a fast, native way to mount their cloud storage as a local drive — without heavy sync, without copying everything locally, and with a login flow as simple as signing in once with their corporate Microsoft account.

No existing tool covers this gap well: rclone requires CLI expertise and has no integrated GUI; onedriver is Linux-only and unmaintained; the official OneDrive client ignores SharePoint document libraries and doesn't exist on Linux. **FileSync** fills this gap as a cross-platform, performant, dead-simple app that mounts OneDrive and configurable SharePoint sites as native drives.

## What Changes

This is a **greenfield application** — everything is new.

- **Microsoft authentication**: OAuth2 authorization code flow with PKCE via Microsoft Entra ID (Azure AD), supporting professional/organizational Microsoft 365 accounts. Silent token refresh for zero-friction daily use.
- **OneDrive mounting**: Mount the user's OneDrive for Business as a local virtual filesystem (drive letter on Windows, mount point on macOS/Linux).
- **SharePoint site mounting**: Browse and select specific SharePoint sites and their document libraries; mount one or more as virtual drives. Site/library selection is configurable and persisted.
- **Virtual filesystem layer (FUSE-based)**: Cross-platform userspace filesystem — FUSE/libfuse on Linux, macFUSE (or FUSE-T) on macOS, WinFSP on Windows — exposing cloud files as native files with read/write support.
- **Intelligent caching**: Multi-tier cache (metadata cache + read-ahead content cache + write-back buffer) for responsive file browsing and editing. Delta queries for efficient change detection.
- **System tray application**: Lightweight background app with system tray icon showing mount status, quick actions (mount/unmount, open folder, sign out), and notifications for sync events or errors.
- **Simple first-run experience**: Launch → sign in with Microsoft → select drives → mount. Three clicks to a working drive.
- **Packaged defaults (build-time configuration)**: The application can be built with pre-filled tenant information, client ID, and default SharePoint mounts baked into the binary via a `build/defaults.toml` file. This enables zero-configuration distribution: the builder edits a TOML file, runs the build, and shares the resulting installer — recipients just sign in, everything is pre-configured. Packaged defaults act as a living base layer that updates can refresh without overwriting user changes.

## Capabilities

### New Capabilities

- `microsoft-auth`: OAuth2/OIDC authentication with Microsoft Entra ID using authorization code flow + PKCE. Token management (access, refresh, silent renewal). Scoped to Microsoft Graph permissions (Files.ReadWrite.All, Sites.Read.All, User.Read).
- `graph-client`: Microsoft Graph API client for OneDrive and SharePoint operations — list drives, browse folders, read/download files, upload (small + chunked upload sessions), create folders, delete, rename/move, delta queries for change tracking.
- `virtual-filesystem`: Cross-platform FUSE-based virtual filesystem that translates filesystem operations (open, read, write, readdir, getattr, create, unlink, rename) into Graph API calls, with inode management and file handle tracking.
- `cache-layer`: Multi-tier caching system — metadata cache (directory listings, file attributes with TTL), read cache (block-level content caching for open files), and write buffer (coalesce writes, flush on close/sync). Delta sync for incremental metadata refresh.
- `sharepoint-browser`: Discovery and selection of SharePoint sites and document libraries available to the authenticated user. Persisted configuration for which sites/libraries to mount.
- `tray-app`: Cross-platform system tray application providing mount lifecycle management (start/stop mounts), status display, account management, settings, and notifications. Minimal UI — runs as a background service.
- `config-persistence`: Application configuration storage — account credentials (encrypted token store), mount points, selected SharePoint sites, cache settings, startup preferences. Cross-platform config directory conventions.
- `packaged-defaults`: Build-time configuration embedding via `build/defaults.toml` → `include_str!` into the binary. Defines tenant ID, client ID, branding (app name), and pre-configured mounts. Two-layer config resolution at runtime: packaged defaults as base, user config as override. Updates refresh the packaged layer automatically. Users can modify or extend on top of packaged defaults via settings.

### Modified Capabilities

_None — this is a greenfield project._

## Impact

- **Dependencies**: Rust toolchain, `fuser`/`easy_fuser` crate (Linux/macOS FUSE), WinFSP SDK + FFI (Windows), `onedrive-api` or direct `reqwest`-based Graph client, `tauri` or native tray integration, OS keychain for token storage.
- **Platform requirements**: FUSE kernel module on Linux (standard), macFUSE or FUSE-T on macOS (user install required), WinFSP on Windows (bundled or user install required).
- **Microsoft 365 tenant**: Requires an Azure AD app registration with appropriate Graph API permissions. Users must belong to a Microsoft 365 organization. Tenant and client ID can be baked into the build via `build/defaults.toml`.
- **Build pipeline**: Builders who want to distribute pre-configured builds need the Rust toolchain and Tauri CLI. They edit `build/defaults.toml` and run `cargo tauri build`.
- **Network**: The app is fundamentally network-dependent for file content; caching mitigates latency but connectivity is required for initial access and writes.
- **Security surface**: Handles OAuth tokens, potentially sensitive corporate documents. Must use OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service/keyring) for token storage. No plaintext credentials.
