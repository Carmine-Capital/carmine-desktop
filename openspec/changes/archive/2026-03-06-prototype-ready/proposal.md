## Why

CloudMount's code is functionally complete — all 6 crates compile, 48+ tests pass, headless and desktop modes are fully implemented. But a developer cloning the repo today cannot actually test the application. The placeholder client ID (`00000000-...`) causes a silent 120s auth timeout, there are no CLI arguments (not even `--help`), system dependencies aren't validated at startup, and there's no runtime way to provide Azure AD credentials without recompiling. The gap between "compiles" and "testable prototype" must be closed.

## What Changes

- **Runtime configuration override chain**: Add a 4-layer config resolution — CLI args > env vars > user config.toml > build/defaults.toml > hardcoded defaults — so developers can provide client_id/tenant_id without recompiling.
- **CLI argument parsing**: Add `clap`-based CLI with `--help`, `--version`, `--client-id`, `--tenant-id`, `--config`, `--log-level`, and `--headless` flags.
- **`.env` file support**: Load `CLOUDMOUNT_*` env vars from a `.env` file in the working directory (or `build/.env`) via `dotenvy`, for developer convenience.
- **Startup validation**: Detect placeholder client ID before attempting auth and exit with a clear error message pointing to setup docs. Check FUSE availability on Linux/macOS at startup.
- **Auth URL fallback**: When no display server is detected (SSH, Docker, CI), print the OAuth URL to stdout instead of silently failing to open a browser.
- **Developer documentation**: Add `DEVELOPING.md` with system deps, Azure AD setup, build commands, and first-run expectations.
- **Fix stale CLAUDE.md**: Update notes that incorrectly say headless mode is a stub — it's fully implemented.
- **Build-time env var injection**: Support `option_env!()` for `client_id`, `tenant_id`, and `app_name` so CI pipelines can inject simple values without managing files.
- **defaults.toml template pattern**: Rename `build/defaults.toml` to `build/defaults.toml.example` (tracked), gitignore `build/defaults.toml`, add `build.rs` that auto-copies `.example` to `defaults.toml` if missing so fresh clones still compile.
- **Build workflows**: Public GitHub CI for generic builds (existing CI extended). Template for private org repo (GitLab/GitHub) that clones the public repo, injects org-specific `defaults.toml` + build-time env vars, and produces branded binaries.

## Capabilities

### New Capabilities

- `developer-experience`: CLI argument parsing, .env file support, startup validation, auth URL fallback for headless environments, developer documentation, and build workflow patterns. Covers the tooling and ergonomics layer that makes the app testable and distributable.

### Modified Capabilities

- `app-lifecycle`: Startup sequence gains pre-flight checks (client ID validation, FUSE availability) and CLI argument parsing before component initialization.
- `microsoft-auth`: OAuth flow gains display detection — prints auth URL to stdout when no browser can be opened.
- `config-persistence`: Configuration resolution chain extends from 2 layers (packaged + user) to 4 layers (CLI > env > user > packaged).
- `packaged-defaults`: Packaged values can now be overridden at runtime via env vars and CLI args, and at build time via `option_env!()`. The `defaults.toml` file is gitignored with an `.example` template. Precedence model extended.

## Impact

- **cloudmount-app**: Major changes — new `clap` CLI, `.env` loading, startup validation, `build.rs` for defaults.toml auto-copy, `option_env!()` for build-time injection, restructured `main()` flow.
- **cloudmount-core**: Config system gains env var and CLI override merging in `EffectiveConfig::build()`.
- **cloudmount-auth**: `oauth.rs` gains display detection and URL-to-stdout fallback.
- **New dependencies**: `clap` (CLI parsing), `dotenvy` (`.env` file loading) — both added to workspace root.
- **New files**: `DEVELOPING.md` (developer guide), `.env.example` (template for credentials), `build/defaults.toml.example` (replaces tracked `defaults.toml`), `cloudmount-app/build.rs` (auto-copy defaults), `docs/org-build-guide.md` (private repo setup guide).
- **Documentation**: `CLAUDE.md` updated to fix stale headless mode notes and document new build patterns.
