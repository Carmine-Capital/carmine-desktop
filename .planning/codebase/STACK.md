# Technology Stack

**Analysis Date:** 2026-03-10

## Languages

**Primary:**
- Rust 1.85 (edition 2024) - Entire workspace (6 crates)

**Secondary:**
- JavaScript (vanilla) - Frontend in `crates/cloudmount-app/dist/` (Tauri desktop UI)
- TOML - Configuration files

## Runtime

**Environment:**
- Tokio 1.50 async runtime (full feature set) — all async operations across all crates
- Platform-specific: FUSE (Linux/macOS), Cloud Files API/CfApi (Windows)

**Package Manager:**
- Cargo - Rust package manager
- Lockfile: `Cargo.lock` present (version 4)

## Frameworks

**Core:**
- Tauri 2 - Desktop application framework (Linux/macOS/Windows)
  - Optional feature-gated: `#[cfg(feature = "desktop")]`
  - Plugins: dialog, notification, updater, process, opener

**Async:**
- Tokio 1.50 with `full` features

**VFS Implementations:**
- `fuser` 0.17 - FUSE filesystem implementation (Linux/macOS)
  - Target-gated: `#[cfg(any(target_os = "linux", target_os = "macos"))]`
- `cloud-filter` 0.0.6 - Windows Cloud Files API bindings
  - Target-gated: `#[cfg(target_os = "windows")]`

**Testing:**
- `wiremock` 0.6 - HTTP mocking for Graph API tests
- Tokio test utilities (`#[tokio::test]`)

**Build/Dev:**
- `tauri-build` 2 - Tauri app compilation
- Cargo (built-in)

## Key Dependencies

**Critical:**
- `reqwest` 0.12 - HTTP client for Microsoft Graph API (features: json, stream)
- `rusqlite` 0.32 - SQLite client (bundled feature) for metadata cache
- `dashmap` 6.1 - Concurrent hashmap for memory cache

**Infrastructure:**
- `hyper` 1.8, `hyper-util` 0.1 - Low-level HTTP for local OAuth callback server
- `http-body-util` 0.1 - HTTP body handling
- `tokio-util` 0.7 - Tokio utilities (CancellationToken)

**Serialization:**
- `serde` 1.0 with `derive` feature
- `serde_json` 1.0 - JSON for Graph API responses
- `toml` 0.8 - Configuration files

**Error Handling:**
- `thiserror` 2.0 - Structured error types in `cloudmount-core::Error`
- `anyhow` 1.0 - Used in `Error::Other` variant for `?` propagation

**Cryptography (Token Storage):**
- `aes-gcm` 0.10.3 - AES-256-GCM encryption for fallback token storage (feature: zeroize)
- `argon2` 0.5.3 - Key derivation function for token encryption (features: alloc, zeroize)
- `sha2` 0.10 - SHA-256 for PKCE code challenge
- `zeroize` 1.8.2 - Memory zeroization of sensitive data (feature: derive)

**Logging/Tracing:**
- `tracing` 0.1 - Structured logging
- `tracing-subscriber` 0.3 - Log subscriber with env-filter support

**OS Integration:**
- `keyring` 3.6 - Secure token storage via OS keychain
- `open` 5.3 - Open URLs/files with system default app (OAuth callback)
- `dirs` 6.0 - Platform-specific directory paths (config, cache)
- `libc` 0.2 - System calls for FUSE (Linux/macOS)
- `windows` 0.58 - Windows API bindings for CfApi setup (selective features)
- `nt-time` 0.8 - Windows NT filetime conversion (feature: chrono)

**Utilities:**
- `chrono` 0.4 - DateTime with serde support
- `uuid` 1.21 - UUIDs with v4 and serde (for inode tracking)
- `base64` 0.22 - Base64 encoding (OAuth, token storage)
- `url` 2.5 - URL parsing (OAuth flows)
- `bytes` 1.11 - Efficient byte handling (file uploads/downloads)
- `futures-util` 0.3 - Future combinators
- `rand` 0.9 - PKCE verifier generation
- `urlencoding` 2.1 - Query parameter encoding

**CLI:**
- `clap` 4 - Command-line argument parsing (derive feature)
- `dotenvy` 0.15 - `.env` file loading for dev

## Configuration

**Environment:**
- `.env.example` template provided in repo root
- Required env vars: `CLOUDMOUNT_CLIENT_ID`, `CLOUDMOUNT_TENANT_ID`
- Optional: `CLOUDMOUNT_APP_NAME`, `CLOUDMOUNT_LOG_LEVEL`, `CLOUDMOUNT_CONFIG`
- Loaded via `dotenvy::dotenv()` in startup

**Build:**
- Workspace root: `Cargo.toml` with `[workspace.dependencies]` (all deps managed centrally)
- Crate-specific `Cargo.toml` files reference workspace deps via `{ workspace = true }`
- Platform gates: `target_os`, `feature = "desktop"` control conditional compilation
- Build integration: `tauri-build` 2 invoked via build.rs (optional, only with `desktop` feature)

**Feature Flags:**
- `desktop` - Enables Tauri, tray, notifications, updater (control desktop GUI)
- All internal crates depend on workspace crates via path dependencies

## Platform Requirements

**Development:**
- Rust 1.85+ with Cargo
- Toolbox container `cloudmount-build` for all cargo commands (see Makefile)
- Platform-specific build targets (Linux/macOS: FUSE tooling; Windows: Microsoft SDK)

**Production:**
- **Linux**: Kernel FUSE support, libfuse-dev (runtime)
- **macOS**: Kernel FUSE support (via osxfuse or similar)
- **Windows**: Windows 10+ with Cloud Files API support (via cloud-filter crate)
- Microsoft 365 organizational account required (v1 limitation)

**Deployment:**
- Desktop app distributed via Tauri bundles (AppImage on Linux, DMG on macOS, MSI on Windows)
- Auto-updater plugin included for in-app updates
- Headless CLI variant possible (without desktop feature)

---

*Stack analysis: 2026-03-10*
