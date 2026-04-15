# Carmine Desktop

This file provides guidance when working with code in this repository.

## Project Overview

Carmine Desktop mounts Microsoft OneDrive and SharePoint document libraries as local filesystems on Linux, macOS, and Windows. Organizational Microsoft 365 accounts only (v1). Rust 2024 workspace + Tauri v2 desktop app with system tray.

| Crate | Stack | Path |
|-------|-------|------|
| **carminedesktop-app** | Tauri 2, Tokio, vanilla-JS UI (`dist/`) | `crates/carminedesktop-app/` |
| **carminedesktop-auth** | OAuth2 PKCE, `keyring` → AES-256-GCM + Argon2id fallback | `crates/carminedesktop-auth/` |
| **carminedesktop-cache** | DashMap (memory) → SQLite (metadata) → disk (blobs) + writeback | `crates/carminedesktop-cache/` |
| **carminedesktop-core** | Shared types (`DriveItem`, `Drive`, `Site`), `thiserror` errors, TOML config | `crates/carminedesktop-core/` |
| **carminedesktop-graph** | Microsoft Graph v1.0 client, `reqwest`, retry/backoff | `crates/carminedesktop-graph/` |
| **carminedesktop-vfs** | FUSE (Linux/macOS, `fuser`) + WinFsp (Windows) with shared `core_ops.rs` | `crates/carminedesktop-vfs/` |

Toolchain: Rust edition 2024, MSRV 1.85. Current version: see `Cargo.toml` `[workspace.package]`.

## Commands

All cargo commands run inside the `carminedesktop-build` toolbox container (see `docs/dev-setup-immutable-linux.md`). The app itself must run on the **host** — FUSE mounts created inside toolbox are invisible outside it.

### Build

```bash
make build            # cargo build --all-targets (headless)
make build-desktop    # cargo build --all-targets --features desktop (Tauri UI)
make build-appimage   # cargo tauri build --features desktop --bundles appimage
```

### Test & lint

```bash
make fmt              # cargo fmt --all
make fmt-check        # cargo fmt --all -- --check (CI mode)
make clippy           # RUSTFLAGS=-Dwarnings, runs core + --features desktop
make test             # cargo test --all-targets
make check            # fmt-check + clippy + test — run before pushing
```

### Release

```bash
./scripts/release.sh 0.2.0                 # bump Cargo.toml + tauri.conf.json, tag, push
./scripts/release.sh 0.2.0 --upload-only   # skip version bump, upload local artifacts
```

Tags matching `v*` trigger `.github/workflows/release.yml` (matrix build: `.deb`, `.AppImage`, `.dmg`, `.nsis`; signs and uploads the `latest.json` updater manifest to `https://static.carminecapital.com/carmine-desktop/latest.json`).

### Run (host)

```bash
./target/release/carminedesktop-app                         # Tauri GUI
./target/release/carminedesktop-app --headless \
    --client-id "..." --tenant-id "..."                     # headless
```

GitHub interactions use the `gh` CLI (not GitHub MCP).

## Code standards

- **Errors**: `thiserror` enum in `carminedesktop-core::Error` (variants: `Auth`, `GraphApi`, `Cache`, `Filesystem`, `Config`, `Network`, `PreconditionFailed`, `Locked`, `Io`, `Other`). Propagate via `carminedesktop_core::Result<T>`. `anyhow::Error` only inside the `Other(#[from] anyhow::Error)` variant.
- **Async**: Tokio throughout. VFS uses `rt.block_on()` because FUSE/WinFsp trait methods are sync.
- **Logging**: `tracing` macros (`info!`, `warn!`, `debug!`, `error!`). Never `println!` / `eprintln!` in library or app code.
- **Dependencies**: declare in workspace root `[workspace.dependencies]`; crates reference `{ workspace = true }`. Do not add direct `[dependencies]` versions to individual crate `Cargo.toml` files.
- **Serde**: per-field `#[serde(rename = "camelCaseField")]` to match Microsoft Graph JSON (e.g. `lastModifiedDateTime`, `eTag`, `@microsoft.graph.downloadUrl`).
- **Platform gates**:
  - `#[cfg(any(target_os = "linux", target_os = "macos"))]` → FUSE
  - `#[cfg(target_os = "windows")]` → WinFsp
  - `#[cfg(feature = "desktop")]` → Tauri UI surface
- **Clippy**: CI runs `RUSTFLAGS=-Dwarnings` with `--all-targets` and `--all-targets --features desktop`. Collapse nested `if`: `if cond { if let Err(e) = f() { ... } }` → `if cond && let Err(e) = f() { ... }`. No suppressed lints without justification.
- **Frontend (vanilla JS, no build step)**:
  - Bind events with `addEventListener` in `.js` files; no inline `onclick=""` (CSP blocks it).
  - Every mutating action calls `showStatus(message, type)` from `dist/ui.js`.
  - IPC via `const { invoke } = window.__TAURI__.core;` then `await invoke('command_name', { ... })`.

## Architecture

- **Shared VFS logic** lives in `crates/carminedesktop-vfs/src/core_ops.rs`. `fuse_fs.rs` (Linux/macOS) and `winfsp_fs.rs` (Windows) implement platform traits and delegate here for cache lookups, Graph calls, writeback, and conflict detection.
- **Mount lifecycle** is orchestrated in `crates/carminedesktop-app/src/main.rs`: `setup_after_launch`, `start_mount_common`, `start_mount` (platform-gated), `stop_mount`, `start_delta_sync`, `graceful_shutdown`.
- **Tauri commands** are `#[tauri::command]` fns in `crates/carminedesktop-app/src/commands.rs`, registered in `invoke_handler!`. They return `Result<T, String>` (Tauri ABI).
- **Cache tiers** (`carminedesktop-cache`): memory (DashMap) → SQLite metadata → disk blobs; writeback buffer in `src/writeback.rs`, delta sync in `src/sync.rs`, upload processor in `carminedesktop-vfs/src/sync_processor.rs`.
- **Auth** (`carminedesktop-auth`): OAuth2 PKCE in `src/oauth.rs` (`generate_pkce()` with SHA-256 challenge). Token storage (`src/storage.rs`) tries OS keyring first, falls back to AES-256-GCM file encrypted with an Argon2id-derived key.
- **Config**: `UserConfig`, `MountConfig`, `EffectiveConfig` in `crates/carminedesktop-core/src/config.rs`. TOML on disk with on-corruption backup. Mount-path templates expand `{home}` / `~/`.
- **CSP** (`dist/*.html`): `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'`.

## Workflow

1. **Plan**: read related code, understand the change scope.
2. **Code**: implement the change.
3. **Format**: `make fmt`.
4. **Lint**: `make clippy` (must be clean — CI enforces `-Dwarnings`).
5. **Test**: `make test`.
6. **Final gate**: `make check` before opening a PR.

## Testing conventions

- Integration tests only — in `crates/<name>/tests/`, not inline `#[cfg(test)]` modules.
- Naming: `test_<module>_<operation>_<scenario>()` (cache/auth) or `<operation>_<scenario>()` (graph/vfs).
- Async: `#[tokio::test]` or `#[tokio::test(flavor = "multi_thread")]`.
- HTTP mocking: `wiremock` — `MockServer::start().await`, `Mock::given(...).respond_with(...).mount(&server).await`.
- Time-sensitive retry tests: `tokio::time::pause()` for determinism.
- File I/O tests: `std::env::temp_dir()` with explicit pre-test cleanup.

## Common mistakes

- Running `make build` or the app itself **inside** toolbox and expecting the FUSE mount to appear on the host — it won't. Build inside, run outside.
- Adding an `onclick="..."` attribute in `dist/*.html` — the CSP silently blocks it. Bind with `addEventListener` in the paired `.js`.
- Letting a `#[tauri::command]` mutate state without a `showStatus(...)` call — users see no feedback.
- Forgetting a platform gate when adding code that only compiles on one OS — CI's cross-platform matrix will fail.
- Mixing `anyhow::Error` into public APIs — only the `Other` variant of `carminedesktop_core::Error` may hold an `anyhow::Error`.
- Adding a dep directly to a crate's `Cargo.toml` — always declare in workspace root and reference with `{ workspace = true }`.
- Bumping version by editing files by hand — use `./scripts/release.sh <version>` so `Cargo.toml` and `tauri.conf.json` stay aligned.

## Do nots

- No suppressed clippy warnings (`#[allow(...)]`) without a comment explaining why.
- No `println!` / `eprintln!` — use `tracing` macros.
- No inline event handlers in HTML (CSP `script-src 'self'`).
- No `#[cfg(test)]` modules inside `src/` — integration tests only.
- No `anyhow::Error` outside `Error::Other`.
- No `cargo` commands run on the host on immutable Linux — use `make` (toolbox) instead.
- Do not skip `make check` before pushing — CI is strict and a red build blocks the release pipeline.
- Do not edit generated or signed artifacts (`latest.json`, `.sig` files, `target/`).
