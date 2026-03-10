# CLOUDMOUNT

CloudMount mounts Microsoft OneDrive and SharePoint document libraries as local filesystems on Linux, macOS, and Windows. Rust 2024 workspace with 6 crates, Tauri desktop app with system tray. Organizational Microsoft 365 accounts only (v1).

## STRUCTURE

```
crates/
├── cloudmount-app/      # Tauri entry point — runtime orchestration, commands, tray, notifications
├── cloudmount-auth/     # OAuth2 PKCE + token storage (keyring → encrypted fallback)
├── cloudmount-cache/    # Multi-tier cache: memory (DashMap) → SQLite → disk + writeback
├── cloudmount-core/     # Shared types (DriveItem, Drive, Site, errors) + config system
├── cloudmount-graph/    # Microsoft Graph API v1.0 client with retry/backoff
└── cloudmount-vfs/      # VFS: FUSE (Linux/macOS), Cloud Files API (Windows)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Shared VFS logic | `cloudmount-vfs/src/core_ops.rs` | Both FUSE and CfApi delegate here — cache lookups, Graph calls, writeback, conflict detection |
| FUSE / CfApi backends | `fuse_fs.rs` / `cfapi.rs` | Implement platform trait methods, delegate to `CoreOps` |
| Mount lifecycle | `cloudmount-app/src/main.rs` | `start_mount`, `stop_mount`, `setup_after_launch`, `graceful_shutdown`, `start_delta_sync` |
| Tauri commands | `cloudmount-app/src/commands.rs` | `#[tauri::command]` fns — register in `invoke_handler!` |
| Frontend | `cloudmount-app/dist/` | Vanilla JS, no build step. Tauri IPC via `window.__TAURI__.core.invoke()` |

## CONVENTIONS

- **Errors**: `thiserror` enum in `cloudmount-core::Error`. Propagate via `cloudmount_core::Result<T>`. `anyhow` for the `Other` variant only.
- **Async**: Tokio throughout. VFS uses `rt.block_on()` because FUSE/CfApi trait methods are sync.
- **Dependencies**: ALL deps in workspace root `[workspace.dependencies]`. Crates reference `{ workspace = true }`.
- **Serde**: `#[serde(rename = "camelCase")]` to match Microsoft Graph API JSON field names.
- **Platform gates**: `#[cfg(any(target_os = "linux", target_os = "macos"))]` for FUSE, `#[cfg(target_os = "windows")]` for CfApi, `#[cfg(feature = "desktop")]` for Tauri UI.

## CONSTRAINTS

- **IMPORTANT: CI enforces zero warnings** — `RUSTFLAGS=-Dwarnings`, clippy runs `--all-targets --all-features`. No suppressed lints without justification.
- **IMPORTANT: No inline event handlers in HTML** — CSP `script-src 'self'` blocks `onclick="..."` etc. Use `addEventListener` in `.js` files only.
- **IMPORTANT: OpenSpec specs are read-only** — never modify files in `openspec/specs/` directly unless explicitly asked. Use the OpenSpec workflow.
- All user-facing actions must provide feedback via `showStatus()` in `ui.js`. Never let a mutating operation complete silently.

## TESTING

- Tests in `crates/<name>/tests/` — integration test convention, NOT inline `#[cfg(test)]` modules.
- Naming: `test_<module>_<operation>_<scenario>()` (cache/auth) or `<operation>_<scenario>()` (graph).
- HTTP mocking: `wiremock` — `MockServer::start().await`, `Mock::given(...).respond_with(...)`.
- Async tests: `#[tokio::test]`, return `cloudmount_core::Result<()>` for `?` propagation.
- Time-sensitive: `tokio::time::pause()` for deterministic retry testing.
- File I/O tests: `std::env::temp_dir()` with explicit cleanup before each test.

## COMMANDS

All cargo commands run inside the `cloudmount-build` toolbox container — see `Makefile` for targets (`make build`, `make test`, `make clippy`, `make check`). The app itself must run on the host (FUSE mounts are invisible inside toolbox).

GitHub interactions use the `gh` CLI (not GitHub MCP).
