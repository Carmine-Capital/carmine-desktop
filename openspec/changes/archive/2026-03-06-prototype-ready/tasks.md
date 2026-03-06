## 1. Dependencies and Project Setup

- [x] 1.1 Add `clap` (with `derive` feature) to workspace `[workspace.dependencies]` in root `Cargo.toml`, then add `clap = { workspace = true }` to `cloudmount-app/Cargo.toml`
- [x] 1.2 Add `dotenvy` to workspace `[workspace.dependencies]` in root `Cargo.toml`, then add `dotenvy = { workspace = true }` to `cloudmount-app/Cargo.toml`
- [x] 1.3 Add `.env` and `build/defaults.toml` to `.gitignore`
- [x] 1.4 Create `.env.example` in repo root with documented `CLOUDMOUNT_CLIENT_ID`, `CLOUDMOUNT_TENANT_ID`, `CLOUDMOUNT_APP_NAME`, `CLOUDMOUNT_LOG_LEVEL`, `CLOUDMOUNT_CONFIG` variables
- [x] 1.5 Rename `build/defaults.toml` to `build/defaults.toml.example` (git mv)
- [x] 1.6 Create `crates/cloudmount-app/build.rs`: if `build/defaults.toml` doesn't exist, copy from `build/defaults.toml.example`; emit `cargo::rerun-if-changed` for the file

## 2. CLI Argument Parsing

- [x] 2.1 Define `CliArgs` struct with clap derive in `cloudmount-app/src/main.rs`: `--client-id` (env=CLOUDMOUNT_CLIENT_ID), `--tenant-id` (env=CLOUDMOUNT_TENANT_ID), `--config` (env=CLOUDMOUNT_CONFIG), `--log-level` (env=CLOUDMOUNT_LOG_LEVEL), `--headless` flag
- [x] 2.2 Add `dotenvy::dotenv().ok()` call at the very start of `main()` before CLI parsing
- [x] 2.3 Parse CLI args via `CliArgs::parse()` after dotenvy, before config loading
- [x] 2.4 Use `--log-level` (or env fallback) to configure the tracing subscriber filter, falling back to RUST_LOG then "info"
- [x] 2.5 Use `--config` path (or env fallback) in place of `config_file_path()` when provided
- [x] 2.6 Wire `--headless` flag: when set with `desktop` feature, call `run_headless()` instead of `run_desktop()`

## 3. Build-time Env Var Injection

- [x] 3.1 Add `option_env!()` constants in `cloudmount-app/src/main.rs`: `BUILD_CLIENT_ID`, `BUILD_TENANT_ID`, `BUILD_APP_NAME` for `CLOUDMOUNT_CLIENT_ID`, `CLOUDMOUNT_TENANT_ID`, `CLOUDMOUNT_APP_NAME`
- [x] 3.2 Update `include_str!` path reference to ensure it still works with the build.rs auto-copy (path unchanged: `build/defaults.toml`)

## 4. Runtime Override Chain

- [x] 4.1 Create `RuntimeOverrides` struct in `cloudmount-app/src/main.rs` with `client_id: Option<String>` and `tenant_id: Option<String>` populated from CLI args
- [x] 4.2 Modify `init_components()` to accept `RuntimeOverrides` and resolve client_id as: `overrides.client_id.or_else(|| BUILD_CLIENT_ID.map(String::from)).or(packaged.client_id().map(String::from)).unwrap_or(DEFAULT_CLIENT_ID.to_string())`, same pattern for tenant_id
- [x] 4.3 Resolve app_name as: `BUILD_APP_NAME.unwrap_or_else(|| packaged.app_name())` and use throughout
- [x] 4.4 Pass `RuntimeOverrides` through to both `run_desktop()` and `run_headless()` code paths

## 5. Startup Pre-flight Validation

- [x] 5.1 Create `preflight_checks()` function that takes the resolved client_id and returns `Result<(), String>`
- [x] 5.2 Implement placeholder client ID check: if client_id equals `00000000-0000-0000-0000-000000000000`, print actionable error to stderr (mention `docs/azure-ad-setup.md`, `--client-id`, `CLOUDMOUNT_CLIENT_ID`, `.env`) and exit(1)
- [x] 5.3 Implement FUSE availability check on Linux: look for `fusermount3` in PATH via `which::which()` or `std::process::Command`, log warning if not found
- [x] 5.4 Implement FUSE availability check on macOS: look for `fusermount` in PATH, log warning if not found
- [x] 5.5 Call `preflight_checks()` in `main()` after config resolution but before `init_components()`

## 6. Auth URL Stdout Fallback

- [x] 6.1 Add display detection helper in `cloudmount-auth/src/oauth.rs`: on Linux check `$DISPLAY` and `$WAYLAND_DISPLAY`; on macOS/Windows always return true
- [x] 6.2 Modify the `open::that()` call site in `oauth.rs`: if no display detected, skip `open::that()` and print auth URL to stdout with "Open this URL in your browser to sign in:" message
- [x] 6.3 If display is detected but `open::that()` fails, fall back to printing auth URL to stdout
- [x] 6.4 Continue waiting on localhost listener regardless of how the URL was presented

## 7. Build Workflow and Org Build Guide

- [x] 7.1 Create `docs/org-build-guide.md` with: overview of the config overlay pattern, step-by-step GitLab private repo setup (defaults.toml + .gitlab-ci.yml), step-by-step GitHub private repo setup (defaults.toml + .github/workflows/build.yml), CI variable configuration (CLIENT_ID masked, TENANT_ID, APP_NAME), how to update to a new CloudMount version (change version tag)
- [x] 7.2 Create template `.gitlab-ci.yml` in `docs/templates/gitlab-ci.yml`: clones public repo at pinned tag, copies defaults.toml, builds with env vars, produces artifact
- [x] 7.3 Create template `.github/workflows/build.yml` in `docs/templates/github-build.yml`: same pattern for GitHub Actions
- [x] 7.4 Update existing `.github/workflows/ci.yml` to ensure it works with the defaults.toml.example → build.rs auto-copy pattern (the build.rs handles it, but verify CI passes)

## 8. Documentation

- [x] 8.1 Create `DEVELOPING.md` with: system dependencies per platform (libfuse3-dev, GTK4/libwebkit2gtk, macFUSE, Tauri CLI), Azure AD setup (link to docs/azure-ad-setup.md), credential configuration (.env, env vars, CLI args with examples), build-time injection (option_env), build commands (headless and desktop), first-run expectations, reference to docs/org-build-guide.md for org builds
- [x] 8.2 Update `CLAUDE.md` to fix stale "headless mode is a stub" notes — headless is fully implemented
- [x] 8.3 Update `CLAUDE.md` COMMANDS section to include new CLI flags and build-time env vars
- [x] 8.4 Update `docs/builder-guide.md` to reference the new config overlay pattern and org-build-guide.md

## 9. Testing

- [x] 9.1 Add test: CLI args parse correctly with all options
- [x] 9.2 Add test: placeholder client ID detected and error message produced
- [x] 9.3 Add test: RuntimeOverrides correctly override packaged defaults in client_id resolution
- [x] 9.4 Add test: build-time option_env values used when no runtime override exists
- [x] 9.5 Add test: display detection helper returns expected values based on env vars
- [x] 9.6 Verify `cargo build --all-targets` passes with zero warnings
- [x] 9.7 Verify `cargo clippy --all-targets --all-features` passes
