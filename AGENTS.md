# Carmine Desktop

This file provides guidance when working with code in this repository.

## Project Overview

Carmine Desktop mounts Microsoft OneDrive and SharePoint document libraries as local filesystems on Windows. Organizational Microsoft 365 accounts only (v1). Rust 2024 workspace + Tauri v2 desktop app with system tray.

| Crate | Stack | Path |
|-------|-------|------|
| **carminedesktop-app** | Tauri 2, Tokio, Solid.js + Vite + TypeScript UI (`frontend/`) | `crates/carminedesktop-app/` |
| **carminedesktop-auth** | OAuth2 PKCE, `keyring` → AES-256-GCM + Argon2id fallback | `crates/carminedesktop-auth/` |
| **carminedesktop-cache** | DashMap (memory) → SQLite (metadata) → disk (blobs) + writeback | `crates/carminedesktop-cache/` |
| **carminedesktop-core** | Shared types (`DriveItem`, `Drive`, `Site`), `thiserror` errors, TOML config | `crates/carminedesktop-core/` |
| **carminedesktop-graph** | Microsoft Graph v1.0 client, `reqwest`, retry/backoff | `crates/carminedesktop-graph/` |
| **carminedesktop-vfs** | WinFsp backend with shared `core_ops.rs` | `crates/carminedesktop-vfs/` |

Toolchain: Rust edition 2024, MSRV 1.85. Current version: see `Cargo.toml` `[workspace.package]`.

Builds only work on Windows (`winfsp-sys` needs the Windows SDK + clang). Development on Linux/macOS is limited to editing sources and running git workflows — CI on `windows-latest` is the source of truth.

## Commands

Run cargo directly on a Windows host (or inside a Windows VM).

### Build

```powershell
cargo build --all-targets --features desktop     # full Tauri UI build
cargo tauri build --features desktop             # produces NSIS installer
```

### Test & lint

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --features desktop -- -D warnings
cargo test --all-targets --features desktop
```

CI enforces the same commands on `windows-latest`. Push and watch the run.

### Release

```bash
./scripts/release.sh 0.2.0                 # bump Cargo.toml + tauri.conf.json, tag, push
./scripts/release.sh 0.2.0 --upload-only   # skip version bump, upload local artifacts
```

Tags matching `v*` trigger `.github/workflows/release.yml` (Windows-only: builds NSIS installer, signs, uploads the `latest.json` updater manifest to `https://static.carminecapital.com/carmine-desktop/latest.json`).

The release script runs on Linux/WSL/Git Bash — it only calls `cargo generate-lockfile` locally (metadata resolution, no compile).

### Run

```powershell
./target/release/carminedesktop-app.exe
```

GitHub interactions use the `gh` CLI (not GitHub MCP).

## Code standards

- **Errors**: `thiserror` enum in `carminedesktop-core::Error` (variants: `Auth`, `GraphApi`, `Cache`, `Filesystem`, `Config`, `Network`, `PreconditionFailed`, `Locked`, `Io`, `Other`). Propagate via `carminedesktop_core::Result<T>`. `anyhow::Error` only inside the `Other(#[from] anyhow::Error)` variant.
- **Async**: Tokio throughout. VFS uses `rt.block_on()` because WinFsp trait methods are sync.
- **Logging**: `tracing` macros (`info!`, `warn!`, `debug!`, `error!`). Never `println!` / `eprintln!` in library or app code.
- **Dependencies**: declare in workspace root `[workspace.dependencies]`; crates reference `{ workspace = true }`. Do not add direct `[dependencies]` versions to individual crate `Cargo.toml` files.
- **Serde**: per-field `#[serde(rename = "camelCaseField")]` to match Microsoft Graph JSON (e.g. `lastModifiedDateTime`, `eTag`, `@microsoft.graph.downloadUrl`).
- **Feature gate**: `#[cfg(feature = "desktop")]` → Tauri UI surface. No platform `#[cfg]` gates — the app is Windows-only.
- **Clippy**: CI runs `RUSTFLAGS=-Dwarnings` with `--all-targets` and `--all-targets --features desktop`. Collapse nested `if`: `if cond { if let Err(e) = f() { ... } }` → `if cond && let Err(e) = f() { ... }`. No suppressed lints without justification.
- **Frontend (Solid.js + Vite + TypeScript, in `crates/carminedesktop-app/frontend/`)**:
  - JSX components under `frontend/src/`; signals/stores for reactive state, `@tanstack/solid-query` for async bootstrapping, per-topic Tauri `listen()` subscriptions for realtime updates.
  - No inline event handlers in `*.html` (CSP `script-src 'self'` blocks them); wire interactions in the `.tsx` components.
  - Every mutating action calls `showStatus(message, kind)` from `frontend/src/components/StatusBar.tsx`.
  - IPC via the typed wrappers in `frontend/src/ipc.ts` — prefer the `api.*` helpers (e.g. `api.saveSettings({...})`) which wrap each `#[tauri::command]`; the bare `invoke<T>(cmd, args)` export is the fallback.
  - `npm --prefix crates/carminedesktop-app/frontend run build` is invoked automatically by `cargo tauri build` (`beforeBuildCommand`).
  - Realtime topics, frontend layout, pin aggregator, upload-progress task and related conventions: `crates/carminedesktop-app/AGENTS.md`.

## Architecture

- **Shared VFS logic** lives in `crates/carminedesktop-vfs/src/core_ops.rs`. `winfsp_fs.rs` implements the WinFsp `FileSystemContext` trait and delegates here for cache lookups, Graph calls, writeback, and conflict detection.
- **Mount lifecycle** is orchestrated in `crates/carminedesktop-app/src/main.rs`: `setup_after_launch`, `start_mount_common`, `start_mount`, `stop_mount`, `start_delta_sync`, `graceful_shutdown`.
- **Tauri commands** are `#[tauri::command]` fns in `crates/carminedesktop-app/src/commands.rs`, registered in `invoke_handler!`. They return `Result<T, String>` (Tauri ABI).
- **Cache tiers** (`carminedesktop-cache`): memory (DashMap) → SQLite metadata → disk blobs; writeback buffer in `src/writeback.rs`, delta sync in `src/sync.rs`, upload processor in `carminedesktop-vfs/src/sync_processor.rs`.
- **Auth** (`carminedesktop-auth`): OAuth2 PKCE in `src/oauth.rs` (`generate_pkce()` with SHA-256 challenge). Token storage (`src/storage.rs`) tries OS keyring first, falls back to AES-256-GCM file encrypted with an Argon2id-derived key.
- **Config**: `UserConfig`, `MountConfig`, `EffectiveConfig` in `crates/carminedesktop-core/src/config.rs`. TOML on disk with on-corruption backup. Mount-path templates expand `{home}` / `~/`.
- **CSP** (`frontend/*.html`): `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost ws://localhost:* wss://localhost:*; object-src 'none'`.

## Workflow

1. **Plan**: read related code, understand the change scope.
2. **Code**: implement the change.
3. **Push**: let CI run `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` on `windows-latest`.
4. **Iterate** on red CI runs until green before opening a PR.

## Testing conventions

- Integration tests only — in `crates/<name>/tests/`, not inline `#[cfg(test)]` modules.
- Naming: `test_<module>_<operation>_<scenario>()` (cache/auth) or `<operation>_<scenario>()` (graph/vfs).
- Async: `#[tokio::test]` or `#[tokio::test(flavor = "multi_thread")]`.
- HTTP mocking: `wiremock` — `MockServer::start().await`, `Mock::given(...).respond_with(...).mount(&server).await`.
- Time-sensitive retry tests: `tokio::time::pause()` for determinism.
- File I/O tests: `std::env::temp_dir()` with explicit pre-test cleanup.

## Common mistakes

- Trying to run `cargo build` on Linux/macOS — `winfsp-sys` won't compile. Use CI on `windows-latest`.
- Adding an `onclick="..."` attribute in `frontend/*.html` — the CSP silently blocks it. Wire interactions in the `.tsx` components via Solid's JSX event props (`onClick={...}`) instead.
- Letting a `#[tauri::command]` mutate state without a `showStatus(...)` call — users see no feedback.
- Mixing `anyhow::Error` into public APIs — only the `Other` variant of `carminedesktop_core::Error` may hold an `anyhow::Error`.
- Adding a dep directly to a crate's `Cargo.toml` — always declare in workspace root and reference with `{ workspace = true }`.
- Bumping version by editing files by hand — use `./scripts/release.sh <version>` so `Cargo.toml` and `tauri.conf.json` stay aligned.

## Do nots

- No suppressed clippy warnings (`#[allow(...)]`) without a comment explaining why.
- No `println!` / `eprintln!` — use `tracing` macros.
- No inline event handlers in HTML (CSP `script-src 'self'`).
- No `#[cfg(test)]` modules inside `src/` — integration tests only.
- No `anyhow::Error` outside `Error::Other`.
- Do not push without CI passing — a red build blocks the release pipeline.
- Do not edit generated or signed artifacts (`latest.json`, `.sig` files, `target/`).
