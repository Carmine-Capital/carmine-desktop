# Technology Stack

**Analysis Date:** 2026-03-18

## Languages

**Primary:**
- Rust 2024 edition — all 6 workspace crates, backend logic, VFS, auth, cache, Graph API client
- Minimum supported Rust version: `1.85` (set in `Cargo.toml` `rust-version`)

**Secondary:**
- JavaScript (Vanilla) — Tauri frontend in `crates/carminedesktop-app/dist/`, no framework, no build step
- HTML/CSS — static UI files for wizard and settings windows
- TOML — user configuration format (`config.toml`)

## Runtime

**Environment:**
- Tokio async runtime (full features) — all async operations, VFS bridges via `rt.block_on()`
- Tauri v2 runtime (desktop feature) — windowing, system tray, IPC, plugin system
- No Node.js runtime required — frontend is pre-built vanilla JS served from `dist/`

**Package Manager:**
- Cargo (Rust) — workspace-level dependency management
- Lockfile: `Cargo.lock` present and committed
- npm — minimal, only used for `yaml` package (tooling dependency, not app runtime)
- Lockfile: `package-lock.json` present

## Frameworks

**Core:**
- Tauri v2 — desktop app framework with system tray, IPC, multi-window, auto-updater
  - Feature-gated: `--features desktop` enables all Tauri plugins
  - Config: `crates/carminedesktop-app/tauri.conf.json`
  - Identifier: `com.carmine-capital.desktop`
- FUSE via `fuser 0.17` — Linux/macOS virtual filesystem (platform-gated)
- WinFsp via `winfsp 0.12` — Windows virtual filesystem (platform-gated)

**Testing:**
- Built-in `#[tokio::test]` — async test harness
- `wiremock 0.6` — HTTP mocking for Graph API and OAuth tests
- No separate test runner; uses `cargo test`

**Build/Dev:**
- `cargo` — build system
- `tauri-build 2` — Tauri build integration (build.rs)
- `clap 4` (derive) — CLI argument parsing with env var support
- Toolbox container `carminedesktop-build` — all cargo commands run inside this container (see `Makefile`)
- Make — build orchestration (`make build`, `make test`, `make clippy`, `make check`)

## Key Dependencies

**Critical (core functionality):**
- `reqwest 0.12` (json, stream features) — HTTP client for Microsoft Graph API and OAuth
- `rusqlite 0.32` (bundled) — SQLite for metadata cache and delta tokens
- `fuser 0.17` — FUSE filesystem implementation (Linux/macOS)
- `winfsp 0.12` — WinFsp filesystem driver (Windows)
- `keyring 3.6` — OS keychain access for secure token storage
- `dashmap 6.1` — concurrent in-memory cache (lock-free hash map)

**Security/Crypto:**
- `aes-gcm 0.10.3` (zeroize) — AES-256-GCM encryption for token storage fallback
- `argon2 0.5.3` (alloc, zeroize) — Argon2id key derivation (64KB memory, 3 iterations)
- `zeroize 1.8.2` (derive) — secure memory zeroing of key material
- `sha2 0.10` — SHA-256 for PKCE code challenge
- `base64 0.22` — URL-safe Base64 for PKCE verifier encoding

**Serialization:**
- `serde 1.0` (derive) — serialization/deserialization framework
- `serde_json 1.0` — JSON handling for Graph API responses
- `toml 0.8` — TOML parsing/serialization for user config files

**Observability:**
- `tracing 0.1` — structured logging throughout all crates
- `tracing-subscriber 0.3` (env-filter, fmt) — log filtering and console output
- `tracing-appender 0.2` — rolling daily log file output

**Infrastructure:**
- `tokio 1.50` (full) — async runtime, timers, signals, networking
- `tokio-util 0.7` (rt) — `CancellationToken` for graceful shutdown and flow cancellation
- `hyper 1.8` (server, http1) — local HTTP server for OAuth PKCE callback
- `hyper-util 0.1` (tokio) — Tokio integration for hyper
- `http-body-util 0.1` — HTTP body utilities for OAuth callback responses
- `bytes 1.11` — efficient byte buffer management for file content
- `futures-util 0.3` — stream combinators for download streaming

**OS Integration:**
- `dirs 6.0` — XDG/platform config/cache/data directory resolution
- `open 5.3` — cross-platform URL/file opening (non-Linux desktop)
- `chrono 0.4` (serde) — date/time handling with serde integration
- `uuid 1.21` (v4, serde) — mount ID generation
- `url 2.5` — URL parsing for OAuth and Graph API
- `urlencoding 2.1` — percent-encoding for Graph API paths

**Windows-specific:**
- `windows 0.58` — Win32 API for message boxes, shell notifications, system info
- `windows-sys 0.59` — low-level Windows syscall bindings for WinFsp
- `winreg 0.55` — Windows Registry access for autostart and WinFsp detection
- `winfsp-sys 0.12` — raw WinFsp FFI bindings
- `nt-time 0.8` (chrono) — Windows NT timestamp conversion

**Tauri Plugins:**
- `tauri-plugin-dialog 2` — native file/message dialogs
- `tauri-plugin-notification 2` — desktop notifications
- `tauri-plugin-updater 2` — auto-update from private server
- `tauri-plugin-process 2` — process management (restart)
- `tauri-plugin-opener 2` — cross-platform URL/file opening
- `tauri-plugin-deep-link 2` — `carminedesktop://` protocol handler
- `tauri-plugin-single-instance 2` (deep-link) — single instance enforcement with argv forwarding

**CLI:**
- `clap 4` (derive, env) — CLI arg parsing with env var binding
- `dotenvy 0.15` — `.env` file loading at startup

## Configuration

**User Configuration:**
- Format: TOML
- Location: `{config_dir}/carminedesktop/config.toml` (resolved via `dirs::config_dir()`)
- Managed by: `crates/carminedesktop-core/src/config.rs` — `UserConfig` / `EffectiveConfig`
- Defaults: hardcoded in `EffectiveConfig::build()` — 5GB cache, 60s sync interval, 60s metadata TTL

**Environment Variables (CLI overrides):**
- `CARMINEDESKTOP_CLIENT_ID` — Azure AD client ID override
- `CARMINEDESKTOP_TENANT_ID` — Azure AD tenant ID override
- `CARMINEDESKTOP_CONFIG` — custom config file path
- `CARMINEDESKTOP_LOG_LEVEL` — log level override
- `RUST_LOG` — tracing env filter (fallback)
- `GH_TOKEN` — GitHub token (CI/tooling only, `.env.example`)

**Hardcoded Constants:**
- Client ID: `8ebe3ef7-f509-4146-8fef-c9b5d7c22252` in `crates/carminedesktop-app/src/main.rs`
- Graph API base: `https://graph.microsoft.com/v1.0` in `crates/carminedesktop-graph/src/client.rs`
- OAuth authority: `https://login.microsoftonline.com/{tenant}/oauth2/v2.0` in `crates/carminedesktop-auth/src/oauth.rs`
- Upload chunk size: 10 MB in `crates/carminedesktop-graph/src/client.rs`
- Small file limit: 4 MB (simple PUT vs upload session) in `crates/carminedesktop-graph/src/client.rs`

**Build Configuration:**
- `crates/carminedesktop-app/tauri.conf.json` — Tauri app config, bundle targets, updater endpoints
- `Cargo.toml` (workspace root) — all dependency versions centralized
- `Makefile` — dev workflow targets run inside toolbox container

**Logging:**
- Daily rolling log files at `{data_dir}/carminedesktop/logs/carminedesktop.log`
- Dual output: stderr (console) + file appender (persisted)
- Default level: `info`, configurable via CLI/env/config

## Platform Requirements

**Development:**
- Rust toolchain ≥ 1.85 (stable channel, `dtolnay/rust-toolchain@stable` in CI)
- Linux: `libfuse3-dev`, `pkg-config`, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
- macOS: `macfuse` (via Homebrew)
- Windows: WinFsp SDK + LLVM (via Chocolatey)
- Toolbox container `carminedesktop-build` for local dev on immutable Linux (Fedora Silverblue/Kinoite)

**Production:**
- Linux: `libfuse3-3` (runtime dependency in .deb), `fusermount3` command
- macOS: macFUSE installed (`/Library/Filesystems/macfuse.fs`)
- Windows: WinFsp driver (bundled in NSIS installer as `winfsp.msi`)
- Bundle targets: `.deb`, `.AppImage` (Linux), `.app`/`.dmg` (macOS), `.nsis` (Windows)

**CI/CD:**
- GitHub Actions — `.github/workflows/ci.yml` (3-platform matrix: ubuntu, macOS, Windows)
- `RUSTFLAGS=-Dwarnings` enforced — zero warnings policy
- Formatting check: `cargo fmt --all -- --check`
- Clippy: `--all-targets` (core) + `--all-targets --features desktop`
- Tests: `cargo test --all-targets`

---

*Stack analysis: 2026-03-18*
