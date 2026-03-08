# System Architecture

## Overview

CloudMount is a cross-platform desktop daemon that mounts Microsoft OneDrive and SharePoint document libraries as native filesystems. It uses platform-specific VFS backends (FUSE on Linux/macOS, Cloud Files API on Windows) with a shared business logic layer, backed by a three-tier cache and an OAuth2-authenticated Graph API client.

## System Context

CloudMount runs as a system tray application. The user signs in via browser OAuth2 (PKCE), selects drives/sites to mount, and the mounts appear as local directories. File operations are transparently forwarded to Microsoft Graph API with local caching.

### Context Diagram

```
┌─────────────────────────────────────────────────────────┐
│                      CloudMount                          │
│                                                         │
│  [System Tray UI] ←→ [App Runtime] ←→ [VFS Backend]    │
│         ↓                   ↓                ↓          │
│  [Tauri Webview]     [Auth Manager]   [FUSE / CfApi]    │
│  (wizard, settings)  [Graph Client]   [Cache Layer]     │
└────────────────────────────────┬────────────────────────┘
                                 │ HTTPS
                    ┌────────────▼────────────┐
                    │  Microsoft Graph API    │
                    │  OneDrive / SharePoint  │
                    └─────────────────────────┘
```

### Users

- **End User**: Desktop user who wants OneDrive/SharePoint files accessible as local folders
- **IT Admin**: Configures tenant ID, custom Azure AD app registration (optional)

### External Systems

- **Microsoft Graph API v1.0**: Drive/file CRUD, delta queries, SharePoint site/library listing
- **Microsoft Identity Platform**: OAuth2 PKCE authorization endpoint, token endpoint
- **OS Keychain**: Secure token storage (Windows Credential Manager, macOS Keychain, Linux Secret Service)

## Architecture Pattern

**Pattern**: Layered multi-crate workspace with platform abstraction
**Rationale**: Clear separation of concerns; platform-specific code isolated behind trait interfaces; shared business logic reusable across FUSE and CfApi backends

## Component Architecture

### Components

#### cloudmount-core
- **Purpose**: Shared foundation — types, errors, config
- **Responsibilities**: `DriveItem`, `Drive`, `Site` types; `Error` enum; `UserConfig`/`EffectiveConfig` system; platform path resolution
- **Dependencies**: None (leaf crate)

#### cloudmount-auth
- **Purpose**: OAuth2 PKCE authentication and token lifecycle
- **Responsibilities**: Browser-based sign-in, token refresh (5-min buffer), keyring storage with AES-256 fallback, sign-out, `try_restore`
- **Dependencies**: cloudmount-core

#### cloudmount-graph
- **Purpose**: Microsoft Graph API v1.0 HTTP client
- **Responsibilities**: Drive/item CRUD, delta queries, upload sessions (chunked for >4MB files), SharePoint site/library discovery, retry with exponential backoff (3 retries, 1s base)
- **Dependencies**: cloudmount-core

#### cloudmount-cache
- **Purpose**: Three-tier cache with writeback and delta sync
- **Responsibilities**: Memory tier (DashMap, 10k entries LRU), SQLite tier (metadata), disk tier (file content), writeback queue, `DeltaSyncTimer` for periodic Graph delta sync, crash recovery
- **Dependencies**: cloudmount-core, cloudmount-graph

#### cloudmount-vfs
- **Purpose**: Virtual filesystem — cross-platform mount/unmount
- **Responsibilities**: FUSE backend (Linux/macOS via `fuser`), CfApi backend (Windows via `cloud-filter`), `CoreOps` shared business logic (cache lookups, Graph calls, writeback, conflict detection), inode table, mount lifecycle
- **Dependencies**: cloudmount-core, cloudmount-graph, cloudmount-cache

#### cloudmount-app
- **Purpose**: Application entry point and runtime orchestrator
- **Responsibilities**: CLI args + .env, preflight checks, Tauri setup (desktop) or headless mode, mount lifecycle (`start_mount`/`stop_mount`), auth degradation, crash recovery, delta sync wiring, system tray + webview UI, Tauri commands, notifications, signal handling
- **Dependencies**: cloudmount-core, cloudmount-auth, cloudmount-vfs

### Component Diagram

```
cloudmount-app (binary)
├── cloudmount-vfs
│   ├── cloudmount-cache
│   │   ├── cloudmount-graph → cloudmount-core
│   │   └── cloudmount-core
│   ├── cloudmount-graph → cloudmount-core
│   └── cloudmount-core
├── cloudmount-auth → cloudmount-core
└── cloudmount-core
```

## Data Flow

File read flow (FUSE read → cache → Graph → disk):

```
User App
  │ read(path)
  ▼
FUSE Kernel ──► fuse_fs.rs::read()
                  │
                  ▼
              CoreOps::fetch_data()
                  │
                  ├─ Memory cache hit? → return bytes
                  │
                  ├─ SQLite cache hit? → hydrate memory → return bytes
                  │
                  └─ Cache miss →
                       GraphClient::download_content()
                         │ HTTPS
                         ▼
                       Graph API
                         │
                         ▼
                       Write to disk cache
                       Populate memory + SQLite
                       return bytes
```

## Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Language | Rust 2024 | Performance + memory safety for filesystem daemon |
| Async runtime | Tokio | Non-blocking I/O throughout |
| Desktop UI | Tauri v2 | System tray + webview (optional `desktop` feature) |
| VFS (Linux/macOS) | fuser (FUSE) | Kernel filesystem integration |
| VFS (Windows) | cloud-filter (CfApi) | Windows Cloud Files API |
| HTTP client | reqwest | Graph API calls |
| Memory cache | DashMap | Concurrent LRU map, 10k entry limit |
| Metadata cache | SQLite (rusqlite) | Persistent item metadata |
| Disk cache | Raw files | File content storage |
| Auth | oauth2 + keyring | PKCE flow + OS keychain |
| Serialization | serde + serde_json | Graph API JSON ↔ Rust types |
| Errors | thiserror | Typed error enum |
| Logging | tracing | Structured logging |

## Non-Functional Requirements

### Performance

- **File read latency**: Memory cache hit < 1ms; cold read depends on network
- **Memory cache**: Max 10,000 entries; evict to 8,000 on overflow
- **Upload chunks**: 10MB chunks for files > 4MB
- **Unmount flush**: Max 30s wait for pending writes

### Security

- No credentials or tokens in source code
- OAuth2 PKCE — no client secret needed
- Tokens stored in OS keychain; AES-256 encrypted file as fallback
- No `innerHTML` with user-controlled data in webview UI

### Scalability

Single-user desktop app — no horizontal scaling concerns. Performance scales with Graph API rate limits and local disk I/O.

## Constraints

- Organizational Microsoft 365 accounts only (v1) — no personal OneDrive
- FUSE requires kernel support (Linux) or macFUSE (macOS)
- Windows CfApi requires Windows 10 1709+ (build 16299+)
- Single official Azure AD client ID (`8ebe3ef7-f509-4146-8fef-c9b5d7c22252`) — no build-time config files

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Performance + safety in filesystem daemon (D1) |
| VFS backends | fuser + cloud-filter | Best-in-class native integration per platform (D2) |
| Graph client | Custom reqwest-based | Needs SharePoint; existing crates insufficient (D3) |
| Cache design | Three-tier (memory/SQLite/disk) | Balance speed, persistence, and disk usage (D4) |
| Auth | OAuth2 PKCE via browser | No client secret; works with MFA/conditional access (D5) |
| Desktop UI | Tauri v2 | Minimal overhead, native tray, webview settings (D6) |
| Config | TOML + OS keychain | Human-readable config; secure secret storage (D7) |
| Client ID | Single hardcoded | No build-time config complexity; one official app (D8) |

---
*Generated by specs.md - fabriqa.ai FIRE Flow*
