## Context

FileSync is a greenfield cross-platform desktop application that mounts Microsoft OneDrive for Business and SharePoint document libraries as native virtual filesystems. There is no existing codebase. The project targets professionals using Microsoft 365 in mixed-OS (Windows, macOS, Linux) environments who need seamless local access to cloud-stored files without heavyweight sync clients.

The existing landscape has significant gaps: the official Microsoft OneDrive client is unavailable on Linux, does not support mounting arbitrary SharePoint sites, and uses full-sync rather than on-demand access. rclone offers CLI-based mounting but lacks an integrated GUI and requires manual configuration. onedriver is Linux-only and unmaintained. No tool combines cross-platform virtual filesystem mounting for both OneDrive and SharePoint with a simple tray-based UX.

**Constraints:**
- Must work on Windows 10/11, macOS 12+, and mainstream Linux distributions (Ubuntu 22+, Fedora 38+, Arch)
- Must use Microsoft Entra ID (Azure AD) for authentication — organizational accounts only (v1: no personal MSA)
- Must not require administrator privileges for day-to-day use (FUSE drivers may require initial one-time install)
- Must handle intermittent network gracefully — cached content remains accessible
- Must comply with Microsoft Graph API rate limits and throttling policies

## Goals / Non-Goals

**Goals:**
- Mount OneDrive for Business as a native filesystem (drive letter or mount point)
- Mount configurable SharePoint document libraries as native filesystems
- Provide near-instant directory browsing via metadata caching
- Support read/write operations with write-back buffering
- Deliver a "sign-in and go" experience — three clicks from install to working drive
- Run as a lightweight background service with system tray control
- Persist configuration and tokens securely across restarts

**Non-Goals:**
- Full bidirectional offline sync (v1 provides read-only offline for cached files; writes require connectivity)
- Personal Microsoft accounts (consumer OneDrive) — organizational only in v1
- SharePoint list items or non-file content — only document libraries
- Real-time collaboration features (co-authoring, presence) — delegated to Office apps
- Mobile platforms (iOS, Android)
- File versioning UI — version history accessible via SharePoint web only
- Bandwidth throttling controls in v1

## Decisions

### D1: Language — Rust

**Decision:** Use Rust as the primary implementation language.

**Rationale:**
- **Performance**: Zero-cost abstractions, no GC pauses — critical for a filesystem daemon where latency matters on every `open()`/`read()`/`readdir()` call.
- **Safety**: Memory safety without GC eliminates an entire class of bugs in long-running daemon processes handling concurrent filesystem requests.
- **Cross-platform**: Compiles natively to Windows, macOS, and Linux with excellent cross-compilation support.
- **Ecosystem**: `fuser` crate provides production-quality FUSE bindings (Linux/macOS); WinFSP has C FFI that Rust handles well. `reqwest` for HTTP, `tokio` for async runtime, `serde` for JSON.
- **Single binary**: No runtime dependency (unlike Go's CGO requirements for FUSE, or Node.js).

**Alternatives considered:**
- *Go*: Strong option (rclone, onedriver use it). `cgofuse` provides cross-platform FUSE. However, CGO adds complexity, GC pauses can affect filesystem responsiveness, and Go's error handling is verbose for the many failure modes in filesystem code.
- *C++*: Maximum performance but lacks memory safety guarantees. Higher maintenance burden for a small team.
- *Node.js/Electron*: Not suitable for a filesystem daemon — runtime overhead, GC pauses, poor fit for low-level FUSE operations.

### D2: Platform VFS Strategy — `fuser` (Linux/macOS) + Cloud Files API (Windows)

**Decision:** Use `fuser` crate for Linux and macOS FUSE operations. Use Windows Cloud Files API (CfApi) via the `cloud-filter` crate (MIT) for Windows. These are fundamentally different paradigms — FUSE is a virtual filesystem, CfApi is a sync engine with placeholder files — so they share core logic (cache, Graph API) but not a unified VFS trait.

**Architecture:**
```
                    ┌──────────────────────────┐
                    │  Shared Core Logic        │
                    │  GraphClient, CacheManager │
                    │  InodeTable, Auth          │
                    └────────────┬─────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                   │
     ┌────────▼────────┐  ┌─────▼──────┐  ┌────────▼──────────┐
     │  fuser impl     │  │ fuser impl │  │  CfApi sync engine │
     │  (Linux)        │  │ (macOS)    │  │  (Windows)         │
     │  virtual FS     │  │ virtual FS │  │  placeholder files │
     └────────┬────────┘  └─────┬──────┘  └────────┬──────────┘
              │                 │                   │
     ┌────────▼───┐  ┌─────────▼────┐  ┌───────────▼──────────┐
     │  libfuse3  │  │  macFUSE /   │  │  cldflt.sys (built   │
     │  (kernel)  │  │  FUSE-T      │  │  into Windows 10+)   │
     └────────────┘  └──────────────┘  └───────────────────────┘
```

**Why CfApi instead of WinFSP:**

CfApi is Microsoft's native cloud sync engine framework (Windows 10 1709+, Oct 2017). It is what **OneDrive, Google Drive, Dropbox, and Nextcloud** all use on Windows. The key architectural difference:

| Aspect | WinFSP (virtual filesystem) | CfApi (sync engine) |
|---|---|---|
| Hot-path I/O (hydrated files) | Always IPC round-trip to your process | Direct NTFS — your process is not involved |
| Directory listing (warm) | IPC round-trip every time | Pure NTFS, zero IPC |
| Write path | IPC to your process per write | Direct NTFS write, no callback |
| Upload detection | Native `write()` callback | `ReadDirectoryChangesW` watcher (external) |
| Explorer integration | Network/removable drive appearance | First-class cloud provider (nav pane, status icons, progress) |
| Driver install | Requires WinFSP installer | Built into Windows (no install) |
| Antivirus compat | Sometimes problematic | Designed for it |

CfApi's performance advantage is structural: once a file is hydrated (downloaded), all subsequent I/O goes through NTFS directly — the sync engine process is not in the hot path. WinFSP always requires an IPC round-trip regardless of cache state.

**CfApi UX features (automatic, built-in):**
- Sync status overlay icons in Explorer (cloud, checkmark, syncing)
- "Make available offline" / "Free up space" context menu
- Download progress bar inline in Explorer
- Windows Storage Sense integration (auto-free disk space)
- File sharing handler (Windows 11 21H2+)
- Branded entry in Explorer navigation pane

**Rust crate: `cloud-filter` (v0.0.6, MIT)**
Safe, idiomatic wrapper around CfApi. Fork of `wincs` by ok-nick, maintained by ho-229. Used by Apache OpenDAL (`cloud_filter_opendal`). ~895 downloads/month. Wraps a stable Windows API surface.

**How CfApi maps to our existing code:**

| Operation | FUSE (Linux/macOS) | CfApi (Windows) |
|---|---|---|
| Directory listing | `fuser::readdir()` callback | `CF_CALLBACK_TYPE_FETCH_PLACEHOLDERS` → `CfCreatePlaceholders()` |
| File read | `fuser::read()` callback | `CF_CALLBACK_TYPE_FETCH_DATA` → `CfExecute()` to feed data |
| File write | `fuser::write()` callback | Direct NTFS write (no callback). Detect via `ReadDirectoryChangesW` |
| Delete/rename | `fuser::unlink/rename()` callback | Detect via `ReadDirectoryChangesW`, then sync to remote |
| Conflict detection | eTag check on upload | Same logic, `CfUpdatePlaceholder()` with `CF_UPDATE_FLAG_MARK_IN_SYNC` |
| Hydration policy | N/A | `CF_HYDRATION_POLICY_PROGRESSIVE` — unblock app early, continue download in background |

**CfApi limitation — no upload callback:** CfApi handles download-on-demand natively but has no built-in write/upload callback. Local changes must be detected via `ReadDirectoryChangesW` (filesystem change watcher). This is the standard approach used by OneDrive, Nextcloud, and all other CfApi-based sync engines.

**Why not a shared VFS trait for FUSE + CfApi:** The paradigms are too different for a meaningful shared trait. FUSE intercepts every filesystem operation synchronously; CfApi only calls back on cache misses (placeholder hydration) and delegates writes entirely to NTFS. Trying to abstract over both would produce a leaky abstraction. Instead, both backends share the core logic layer (`GraphClient`, `CacheManager`, `InodeTable`) directly.

**Alternatives considered:**
- *WinFSP via `winfsp-wrs`*: Works, but every I/O operation requires IPC to your process even for cached/hydrated files. Network drive UX in Explorer. Requires WinFSP installer. Used by developer tools (rclone, SSHFS) but no major consumer cloud product uses WinFSP.
- *WinFSP via `winfsp` (GPL-3.0)*: Adds native async dispatch, but GPL license is incompatible. Same UX limitations as `winfsp-wrs`.
- *easy_fuser*: Higher-level FUSE wrapper. Not relevant to Windows.
- *Custom CfApi FFI via `windows` crate*: The `windows` crate has full CfApi coverage (`Win32::Storage::CloudFilters`), but raw `unsafe` FFI. The `cloud-filter` crate wraps this safely.

### D3: Graph API Client — Custom `reqwest`-based client with `onedrive-api` crate for reference

**Decision:** Build a custom Microsoft Graph API client using `reqwest` + `tokio`, with `onedrive-api` crate as reference/partial dependency for OneDrive-specific types.

**Rationale:**
- `onedrive-api` (by oxalica) provides well-typed OneDrive operations via Graph API but lacks SharePoint site/library operations.
- We need both OneDrive and SharePoint endpoints in a unified client. Building on `reqwest` directly gives full control over SharePoint-specific API calls.
- Reuse `onedrive-api`'s type definitions (DriveItem, Permission, etc.) where possible.

**Key API surface:**
| Operation | Endpoint |
|---|---|
| List user's drive | `GET /me/drive` |
| List children | `GET /drives/{id}/items/{item-id}/children` |
| Get item metadata | `GET /drives/{id}/items/{item-id}` |
| Download content | `GET /drives/{id}/items/{item-id}/content` |
| Upload small (<4MB) | `PUT /drives/{id}/items/{item-id}/content` |
| Upload large (session) | `POST /drives/{id}/items/{item-id}/createUploadSession` |
| Create folder | `POST /drives/{id}/items/{parent-id}/children` |
| Delete item | `DELETE /drives/{id}/items/{item-id}` |
| Move/rename | `PATCH /drives/{id}/items/{item-id}` |
| Delta query | `GET /drives/{id}/root/delta` |
| List sites | `GET /sites?search={query}` |
| Get site drives | `GET /sites/{site-id}/drives` |

### D4: Caching Architecture — Three-tier with SQLite metadata store

**Decision:** Implement a three-tier cache: in-memory metadata (hot), SQLite metadata (warm), disk content cache (cold).

```
┌────────────────────────────────────────────────────┐
│                    VFS Layer                        │
└──────────────────────┬─────────────────────────────┘
                       │
┌──────────────────────▼─────────────────────────────┐
│  Tier 1: In-Memory Metadata Cache                  │
│  - HashMap<InodeId, FileAttr>                      │
│  - HashMap<InodeId, Vec<DirEntry>>                 │
│  - TTL: 60 seconds (configurable)                  │
│  - LRU eviction at 10k entries                     │
└──────────────────────┬─────────────────────────────┘
                       │ miss
┌──────────────────────▼─────────────────────────────┐
│  Tier 2: SQLite Metadata Store                     │
│  - Table: items (inode, parent, name, size, mtime) │
│  - Table: delta_tokens (drive_id, token)           │
│  - Table: sync_state (item_id, local_hash, etag)   │
│  - Persists across restarts                        │
│  - Updated via delta queries                       │
└──────────────────────┬─────────────────────────────┘
                       │ miss or content needed
┌──────────────────────▼─────────────────────────────┐
│  Tier 3: Disk Content Cache                        │
│  - Files stored at: <cache_dir>/<drive_id>/<hash>  │
│  - Max size: configurable (default 5 GB)           │
│  - LRU eviction when full                          │
│  - Block-level caching for partial reads           │
│  - Write-back buffer (flush on close/sync)         │
└────────────────────────────────────────────────────┘
```

**Rationale:**
- Three tiers balance speed, persistence, and storage. In-memory for fast `getattr`/`readdir` (most common FUSE ops). SQLite for restart resilience. Disk for file content.
- SQLite is embedded, requires no external service, and handles concurrent reads efficiently (WAL mode).
- Delta queries keep SQLite metadata fresh without re-listing entire drives.

### D5: Authentication — OAuth2 Authorization Code Flow with PKCE via system browser

**Decision:** Use OAuth2 Authorization Code Flow with PKCE, launching the system browser for login and listening on a localhost redirect URI.

**Flow:**
```
User clicks "Sign In"
       │
       ▼
App opens system browser → Microsoft login page
       │
User authenticates (supports MFA, Conditional Access)
       │
       ▼
Browser redirects to http://localhost:{port}/callback?code=XXX
       │
       ▼
App exchanges code for tokens (access + refresh)
       │
       ▼
Tokens stored in OS keychain (encrypted)
       │
       ▼
Access token used for Graph API calls
       │
       ▼
Token auto-refreshed before expiry using refresh token
```

**Rationale:**
- PKCE is the recommended flow for public clients (desktop apps) per Microsoft/OAuth2 best practices.
- System browser supports SSO, MFA, Conditional Access policies — everything the user's IT admin requires.
- Device Code Flow is an alternative but is clunky UX (type code on another device). Reserve as fallback for headless/SSH scenarios.

**Scopes required:** `User.Read`, `Files.ReadWrite.All`, `Sites.Read.All`, `offline_access`

### D6: UI Strategy — Tauri for system tray + webview settings

**Decision:** Use Tauri v2 for the system tray application and settings UI. The Rust backend runs the FUSE daemon and Graph client; the frontend is a lightweight HTML/CSS/JS webview for settings and first-run wizard.

**Rationale:**
- Tauri is Rust-native — the tray app and FUSE daemon share the same process/memory space. No IPC overhead.
- Tiny bundle size (~5MB vs ~150MB for Electron).
- System tray support on all three platforms via Tauri's tray API.
- The settings/wizard UI is infrequently shown — a webview is fine for occasional use.
- First-run wizard and settings are simple forms — no need for a heavy frontend framework. Vanilla JS or Svelte.

**Alternatives considered:**
- *Electron*: Too heavy (150MB+ runtime) for what is primarily a background daemon.
- *Pure CLI*: Doesn't meet the "super simple" UX requirement. CLI mode will exist as secondary interface.
- *Native GUI per platform*: 3x development effort, not justified for a settings panel.

### D7: Configuration Storage — TOML config + OS keychain for secrets

**Decision:** Use TOML files for non-sensitive configuration and OS keychain for OAuth tokens.

**Config file:** `~/.config/filesync/config.toml` (Linux/macOS) / `%APPDATA%\FileSync\config.toml` (Windows)

```toml
[general]
auto_start = true
log_level = "info"
cache_dir = "~/.cache/filesync"
cache_max_size = "5GB"
metadata_ttl_secs = 60

[[mounts]]
name = "My OneDrive"
type = "onedrive"
account_id = "acc-001"
drive_id = "b!xxxxx"
mount_point = "/home/user/OneDrive"
enabled = true

[[mounts]]
name = "Project Docs"
type = "sharepoint"
account_id = "acc-001"
site_id = "contoso.sharepoint.com,guid1,guid2"
drive_id = "b!yyyyy"
mount_point = "/home/user/SharePoint/ProjectDocs"
enabled = true
```

**Secrets:** Stored via `keyring` crate → Windows Credential Manager / macOS Keychain / Linux Secret Service (GNOME Keyring / KWallet).

### D8: Build-Time Configuration — `build/defaults.toml` embedded via `include_str!`

**Decision:** Support pre-configured builds by embedding a `build/defaults.toml` file into the binary at compile time. At runtime, the application merges this packaged layer with the user's on-disk config to produce the effective configuration.

**Build-time mechanism:**
```
Builder's workflow:

  1. Edit build/defaults.toml
     ├─ tenant_id, client_id
     ├─ branding (app_name)
     └─ [[mounts]] (pre-configured drives)

  2. cargo tauri build
     └─ build.rs reads defaults.toml
        └─ include_str!() bakes it into binary

  3. Distribute the installer
     └─ Recipients install, sign in, done
```

```rust
// build.rs
fn main() {
    let defaults_path = concat!(env!("CARGO_MANIFEST_DIR"), "/build/defaults.toml");
    if std::path::Path::new(defaults_path).exists() {
        println!("cargo:rerun-if-changed=build/defaults.toml");
    }
}

// src/config.rs — the packaged TOML, or empty string if not present
const PACKAGED_DEFAULTS: &str = include_str!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/build/defaults.toml")
);
```

**Runtime config resolution (two-layer merge):**
```
┌────────────────────────────────────────────────────────────┐
│              EFFECTIVE CONFIG (runtime)                     │
│                                                            │
│  For each setting:                                         │
│    effective[key] = user_config[key] ?? packaged[key]      │
│                                                            │
│  For mounts (union by stable ID):                          │
│    effective_mounts = packaged_mounts ∪ user_mounts        │
│    - Packaged mounts always present (unless dismissed)     │
│    - User mounts added on top                              │
│    - If same ID exists in both, user values override       │
│      packaged values for that mount                        │
│                                                            │
│  ┌────────────────────────────┐                            │
│  │ User config (on disk)      │  ← user changes, persists │
│  │ ~/.config/filesync/        │    across updates          │
│  │ config.toml                │                            │
│  └─────────────┬──────────────┘                            │
│                │ overrides                                  │
│  ┌─────────────▼──────────────┐                            │
│  │ Packaged defaults          │  ← baked in binary,        │
│  │ (inside binary)            │    refreshed on update     │
│  └────────────────────────────┘                            │
└────────────────────────────────────────────────────────────┘
```

**The `build/defaults.toml` format:**
```toml
[tenant]
id = "contoso.onmicrosoft.com"
client_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"

[branding]
app_name = "Contoso Drive"

[defaults]
auto_start = true
cache_max_size = "5GB"
sync_interval_secs = 60

[[mounts]]
id = "onedrive"
name = "OneDrive"
type = "onedrive"
mount_point = "{home}/Contoso/OneDrive"
enabled = true

[[mounts]]
id = "project-hub"
name = "Project Hub"
type = "sharepoint"
site_id = "contoso.sharepoint.com,guid1,guid2"
drive_id = "b!yyyyy"
library_name = "Documents"
mount_point = "{home}/Contoso/ProjectHub"
enabled = true
```

**Mount tracking across updates:** Each packaged mount has a stable `id` field. When the app updates:
- A packaged mount with a known ID that the user hasn't modified → gets the new packaged values automatically.
- A packaged mount the user modified (e.g., changed mount point) → user's overrides are preserved.
- A packaged mount the user explicitly dismissed → stays dismissed, unless the mount definition changed substantially in the new version (different `id`).
- A user-added mount → untouched by updates.

**Rationale:**
- `include_str!` is zero-cost — no file I/O at runtime, no risk of missing provision files.
- The Ockam project (Rust CLI) uses this exact pattern for white-label builds in production: env vars read by `build.rs`, injected as compile-time constants.
- Two-layer merge is simple to reason about: "packaged is the base, user changes always win."
- A generic build (empty `defaults.toml`) works identically to the self-service flow — the wizard detects no packaged tenant and shows the full flow.

**Alternatives considered:**
- *Sidecar provision file*: A separate `provision.toml` shipped alongside the binary. Simpler for IT admins who don't want to build from source, but adds a file to manage and can be lost/modified. Could be added later as Approach B (read from well-known system paths) without changing the core merge logic.
- *MDM/registry-based config*: Enterprise standard (Slack, Docker, Zoom use it). Overkill for the primary use case (send installer to coworkers) but the merge architecture supports adding a third layer later.

### D9: Windows Development Workflow — `#[cfg]` gates + cross-check + CI testing

**Decision:** Develop Windows CfApi code on Linux behind `#[cfg(windows)]` gates. Use a three-tier validation strategy: local cross-compilation checks, CI-based build and test, and optional local Windows VM for runtime debugging.

**Workflow:**
```
┌─────────────────────────────────────────────────────────────┐
│  Tier 1: Local (Linux)                                      │
│  cargo-xwin check --target x86_64-pc-windows-msvc           │
│  → catches type errors, API mismatches (seconds)            │
├─────────────────────────────────────────────────────────────┤
│  Tier 2: CI (GitHub Actions windows-latest)                 │
│  cargo build → cargo test (CfApi is built into Windows)     │
│  → real compilation, linking, and functional tests (~5 min) │
├─────────────────────────────────────────────────────────────┤
│  Tier 3: Local VM (WinBoat or QEMU, optional)               │
│  Real Windows 10+ → sync root registration, hydration, I/O  │
│  → runtime debugging, Explorer integration testing          │
└─────────────────────────────────────────────────────────────┘
```

**Rationale:**
- CfApi (`cldflt.sys`) is a Windows kernel minifilter — it cannot run under Wine or in Linux containers. CI with real Windows runners is the only automated testing path.
- `cargo-xwin` provides fast local feedback by cross-compiling to the MSVC target using Microsoft SDK headers downloaded via `xwin`. The `cloud-filter` crate depends on the `windows` crate which provides full CfApi bindings, so no additional SDK install is needed.
- Unlike WinFSP, CfApi requires no driver installation — it is built into Windows 10 1709+. CI setup is simpler (no `choco install` step).
- WinBoat (https://winboat.app/) wraps QEMU/KVM in a user-friendly GUI with automatic filesystem sharing (Linux home dir mounted in Windows), making the edit-on-Linux → test-on-Windows loop ergonomic. Raw QEMU/virt-manager is the alternative for scenarios needing VM snapshots or kernel debugging.
- Tier 3 is optional for scaffolding tasks (callback implementation) but becomes essential for integration testing (sync root registration, Explorer UX, placeholder hydration).

**Alternatives considered:**
- *Wine*: Not viable — CfApi is a kernel minifilter (`cldflt.sys`). Wine cannot emulate Windows kernel filesystem filters.
- *Docker with Windows cross-compilation*: Adds container complexity for the same result as `cargo-xwin` directly. No benefit over Tier 1.
- *CI-only (no local cross-check)*: Viable but slow feedback loop (push → wait 5 min → read logs). `cargo-xwin check` catches most errors in seconds.

## Risks / Trade-offs

| Risk | Impact | Mitigation |
|---|---|---|
| **FUSE driver installation** required on Linux/macOS | Users must install macFUSE on macOS before FileSync works; FUSE is standard on Linux | Guide macFUSE install on macOS; investigate FUSE-T as kext-free alternative. Windows uses CfApi (built-in, no install needed). |
| **macFUSE is not open-source** and requires paid license for redistribution | Distribution issues on macOS | Support FUSE-T as primary macOS backend (open-source, no kext). Fall back to macFUSE if installed. |
| **Microsoft Graph API rate limiting** (429 throttling) | Heavy directory browsing could trigger throttling | Aggressive metadata caching (Tier 1 + 2), exponential backoff with jitter, delta queries to minimize API calls, request batching ($batch endpoint). |
| **CfApi integration on Windows-only** | Cannot develop or test CfApi code on Linux natively; CfApi is a Windows kernel minifilter | Use `cloud-filter` crate (MIT, v0.0.6). Develop behind `#[cfg(windows)]` gates with `cargo-xwin check` for local type-checking. CI tests on GitHub Actions `windows-latest` (CfApi is built-in, no install needed). Local runtime testing via WinBoat or QEMU Windows VM when needed. |
| **`cloud-filter` crate maturity** | v0.0.6, relatively young crate (~895 downloads/month) | Wraps a stable, well-documented Windows API. Used by Apache OpenDAL. Fork of `wincs` with active maintainer. If the crate stalls, fallback to raw `windows` crate FFI (same underlying API). |
| **CfApi write detection via `ReadDirectoryChangesW`** | No native upload callback — must use filesystem change watcher to detect local writes | Standard approach used by OneDrive, Nextcloud, Google Drive, Dropbox. Well-understood pattern. Our existing delta sync logic (`DeltaSyncTimer`) already handles change detection on the remote side — local change detection is the symmetric counterpart. |
| **Large file handling** over network | Editing large files (>100MB) will be slow | Stream-on-demand (don't download entire file for `read()` at offset). Chunked uploads via upload sessions. Warn users for very large files. |
| **Token/auth failures** mid-session | Filesystem calls fail unexpectedly | Silent token refresh on 401. If refresh fails, switch mount to read-only (cached) mode and notify user to re-authenticate. Never crash the FUSE daemon. |
| **Concurrent write conflicts** | Two users editing same SharePoint file | Optimistic concurrency using `eTag`. On conflict: save local copy as `.conflict`, download remote version, notify user. SharePoint checkout/checkin as opt-in for explicit locking. |
| **Build-time config requires Rust toolchain** | Builders must have cargo/Tauri CLI to produce pre-configured builds | Document the build process clearly. Provide a GitHub Actions template so builders can fork the repo and configure via CI without local Rust setup. |
| **Packaged defaults diverge from user config** | After many updates, the gap between packaged and user state may be confusing | Settings UI clearly indicates which values come from packaged defaults vs user overrides. "Reset to Default" reverts a setting to the packaged value. |

## Open Questions

1. **macOS: macFUSE vs FUSE-T as default** — FUSE-T avoids kernel extensions but uses NFS under the hood which adds overhead. Need benchmarks on typical workload (directory listing, small file reads).
2. **Multi-account support in v1?** — The proposal mentions it as a capability in `config-persistence`. Should v1 support multiple Microsoft accounts or just one? Recommendation: single account in v1, design data model for multi-account from the start.
3. **Azure AD App Registration** — Resolved by D8: the `client_id` is set in `build/defaults.toml`. Each builder registers their own Azure AD app for their tenant and bakes the client_id into their build. Generic builds ship with a FileSync-owned app registration as fallback.
4. **Delta query polling interval** — Default of 60 seconds seems reasonable. Should this be adaptive (faster when user is actively browsing, slower when idle)?
5. **Windows mount style** — Resolved by D2: CfApi uses sync roots registered at folder paths (e.g., `%USERPROFILE%\FileSync\<mount-name>`), not drive letters. The sync root appears as a first-class entry in Explorer's navigation pane. This is the same model OneDrive uses.
