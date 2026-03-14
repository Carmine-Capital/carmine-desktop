## Context

carminedesktop is code-complete across all 6 crates with 48+ passing tests. However, the gap between "compiles" and "testable prototype" is significant. The only way to provide Azure AD credentials is editing `build/defaults.toml` and recompiling. The placeholder `DEFAULT_CLIENT_ID` (`00000000-...`) causes a silent 120s auth timeout. There are no CLI arguments, no startup validation, and no fallback for headless environments without a display server.

Currently, configuration flows through a two-layer system: `build/defaults.toml` (compile-time, embedded via `include_str!`) merges with `~/.config/carminedesktop/config.toml` (runtime user overrides) to produce `EffectiveConfig`. Tenant credentials (`client_id`, `tenant_id`) exist only in `PackagedDefaults` — there is no runtime override mechanism.

## Goals / Non-Goals

**Goals:**
- A developer can clone the repo, provide credentials via env vars or `.env`, and run the app without recompiling
- Invalid configuration (placeholder client ID, missing FUSE) is caught at startup with actionable error messages
- Headless mode works in environments without a display (SSH, Docker, CI)
- Clear developer documentation covers the end-to-end setup process

**Non-Goals:**
- Changing the two-layer config merge architecture (we're extending it, not replacing it)
- Multi-account support (single account per AuthManager remains)
- Native installer packaging (.deb, .msi, .dmg generation — that's a separate change)
- GUI for entering client_id/tenant_id (dev-only concern, env vars suffice)

## Decisions

### D1: Four-layer config resolution

**Decision**: Extend config resolution from 2 layers to 4 layers with this precedence:

```
CLI args (highest) → env vars → user config.toml → build/defaults.toml → hardcoded defaults (lowest)
```

**Rationale**: The two-layer system (packaged + user) serves the org builder use case well, but developers need to iterate without recompiling. Environment variables are the standard approach for runtime overrides in CLI tools. CLI args take highest precedence for one-off testing.

**Alternative considered**: Adding credentials to `UserConfig` (config.toml). Rejected because tenant credentials are deployment-level configuration, not user preferences. Mixing them in the user config would also mean they survive `Reset All to Defaults`, which is confusing.

**Implementation**: Add a `RuntimeOverrides` struct populated from CLI + env vars. Pass it into `init_components()` alongside `PackagedDefaults`. The override chain is: `overrides.client_id.or(packaged.client_id()).unwrap_or(DEFAULT_CLIENT_ID)`.

### D2: clap for CLI parsing

**Decision**: Use `clap` with derive macros for CLI argument parsing.

**Rationale**: `clap` is the Rust ecosystem standard, provides `--help`/`--version` for free, and supports env var fallback natively via `#[arg(env = "carminedesktop_CLIENT_ID")]`. This eliminates the need for separate env var parsing logic.

**Alternative considered**: Manual `std::env::args()` parsing. Rejected — too much boilerplate for no benefit, and we'd miss `--help` generation.

**CLI structure**:
```
carminedesktop-app [OPTIONS]

Options:
  --client-id <ID>       Azure AD client ID [env: carminedesktop_CLIENT_ID]
  --tenant-id <ID>       Azure AD tenant ID [env: carminedesktop_TENANT_ID]
  --config <PATH>        Config file path [env: carminedesktop_CONFIG]
  --log-level <LEVEL>    Log level (trace/debug/info/warn/error) [env: carminedesktop_LOG_LEVEL]
  --headless             Run without GUI (even if desktop feature is enabled)
  -h, --help             Print help
  -V, --version          Print version
```

### D3: dotenvy for .env file loading

**Decision**: Use `dotenvy` to load `.env` file from the current working directory before CLI parsing.

**Rationale**: `.env` files are the standard development pattern. `dotenvy` is lightweight (~200 lines), loads env vars before `clap` parses them (so `.env` values appear as env vars to clap's `env` attribute), and silently skips if no `.env` file exists. No config for production builds — the file simply isn't there.

**Alternative considered**: Custom `.env` parsing, or `build/.env` only. Rejected — `dotenvy` handles edge cases (quoting, multiline, comments) and is battle-tested.

**Load order**: `dotenvy::dotenv().ok()` called at the very start of `main()`, before `clap` parsing. This means `.env` values are available as env vars for clap's `#[arg(env = "...")]` integration.

### D4: Startup pre-flight checks

**Decision**: Add a `preflight_checks()` function that runs after config resolution but before component initialization. It validates:

1. **Client ID** — if it equals the placeholder `00000000-...`, print a clear error with instructions and exit(1)
2. **FUSE availability** (Linux/macOS) — check if `fusermount3` is in PATH; if not, warn (don't exit — user might not need mounts yet)
3. **Display availability** (headless) — check `$DISPLAY` (X11) / `$WAYLAND_DISPLAY` on Linux; set a flag for auth URL fallback

**Rationale**: These are the top three first-run failures. Catching them early with clear messages saves debugging time.

**Alternative considered**: Making these runtime errors instead of pre-flight. Rejected — a 120s auth timeout for a bad client ID is unacceptable developer experience.

### D5: Auth URL stdout fallback

**Decision**: In `oauth.rs`, when `open::that()` fails (or when no display server is detected), print the auth URL to stdout and wait for the callback.

**Rationale**: This enables headless auth in SSH sessions, Docker, and CI. The user copies the URL to any browser (even on a different machine), completes auth, and the localhost callback still works because the redirect goes to `http://localhost:{port}/callback` on the machine running carminedesktop.

**Implementation**: The `open::that()` call already has error handling. Extend it to:
1. Check `$DISPLAY` / `$WAYLAND_DISPLAY` (Linux) or always attempt `open::that()` (macOS/Windows)
2. If no display or `open::that()` fails, print: `Open this URL in your browser to sign in:\n\n  {auth_url}\n\nWaiting for authentication...`
3. Continue waiting on the localhost listener as before

### D6: .env.example as documentation

**Decision**: Ship a `.env.example` file in the repo root with documented variables and placeholder values.

**Rationale**: This is the standard pattern for documenting available env vars. Developers copy it to `.env` and fill in their values.

```
# carminedesktop Development Configuration
# Copy this file to .env and fill in your values
# See docs/azure-ad-setup.md for how to obtain these

carminedesktop_CLIENT_ID=your-client-id-here
carminedesktop_TENANT_ID=your-tenant-id-here
# carminedesktop_LOG_LEVEL=debug
# carminedesktop_CONFIG=/path/to/custom/config.toml
```

### D7: Build-time env vars via option_env!()

**Decision**: Use `option_env!()` for `carminedesktop_CLIENT_ID`, `carminedesktop_TENANT_ID`, and `carminedesktop_APP_NAME` at compile time, as an additional layer between `defaults.toml` and the hardcoded defaults.

**Rationale**: CI/CD pipelines (GitHub Actions, GitLab CI) have native secret/variable management. `option_env!()` lets CI inject simple values without creating files. This is the cleanest path for credentials — secrets never touch a file, not even temporarily.

**Implementation**: In `main.rs`, add:
```rust
const BUILD_CLIENT_ID: Option<&str> = option_env!("carminedesktop_CLIENT_ID");
const BUILD_TENANT_ID: Option<&str> = option_env!("carminedesktop_TENANT_ID");
const BUILD_APP_NAME: Option<&str> = option_env!("carminedesktop_APP_NAME");
```

Resolution chain for `client_id` becomes:
```
CLI arg → runtime env var → build-time option_env → defaults.toml → DEFAULT_CLIENT_ID
```

Note: runtime `carminedesktop_CLIENT_ID` (via clap env) and build-time `carminedesktop_CLIENT_ID` (via option_env) use the same variable name. This is fine because: at build time, if the var is set, it bakes in; at runtime, clap reads the current env. If both exist, the runtime value wins (clap parses it as a runtime override before we check the baked-in constant).

**Alternative considered**: Separate env var names for build-time (e.g., `carminedesktop_BUILD_CLIENT_ID`). Rejected — adds confusion. The same var name works naturally: set it during `cargo build` for embedding, set it at runtime for override.

### D8: defaults.toml.example + build.rs auto-copy

**Decision**: Rename `build/defaults.toml` to `build/defaults.toml.example` (tracked in git), gitignore `build/defaults.toml`, and add a `build.rs` in `carminedesktop-app` that copies `.example` to `defaults.toml` if the latter doesn't exist.

**Rationale**: Prevents accidental commit of org credentials. The `.example` file serves as documentation. `build.rs` ensures fresh clones compile without manual steps (the `include_str!` in `main.rs` still references `build/defaults.toml`, which `build.rs` creates from the template).

**build.rs** (~10 lines):
```rust
fn main() {
    let defaults = concat!(env!("CARGO_MANIFEST_DIR"), "/../../build/defaults.toml");
    let example = concat!(env!("CARGO_MANIFEST_DIR"), "/../../build/defaults.toml.example");
    if !std::path::Path::new(defaults).exists() {
        std::fs::copy(example, defaults).expect("failed to copy defaults.toml.example");
    }
    println!("cargo::rerun-if-changed={defaults}");
}
```

**Alternative considered**: Generating the const in build.rs instead of using `include_str!`. Rejected — more complex, and `include_str!` is already used throughout the codebase.

### D9: Config overlay pattern for org builds

**Decision**: Document and support a "config overlay" pattern where a private org repo (GitLab or GitHub) clones the public carminedesktop repo, injects org-specific config, and builds.

**Pattern**:
```
github.com/nyxa/carminedesktop (public)
  └── Source code, defaults.toml.example, generic CI

gitlab.company.com/you/carminedesktop-build (private, tiny)
  ├── defaults.toml          ← SharePoint mount definitions
  ├── .gitlab-ci.yml         ← clones public repo, injects config, builds
  └── CI Variables: CLIENT_ID (masked), TENANT_ID, APP_NAME
```

**Rationale**: Clean separation of concerns. Public repo stays generic. Org config is private. No fork sync burden — the private repo pins to a version tag and doesn't carry source code.

The private repo's CI:
1. Clones public repo at a specific tag
2. Copies `defaults.toml` into `build/`
3. Builds with `carminedesktop_CLIENT_ID` and `carminedesktop_TENANT_ID` env vars (picked up by `option_env!()`)
4. Produces branded binary

**Deliverable**: A `docs/org-build-guide.md` with step-by-step instructions and a template `.gitlab-ci.yml` and `.github/workflows/build.yml` for org builds.

**Alternative considered**: Private fork of the public repo. Rejected — fork maintenance is a burden. The config overlay pattern avoids carrying source code in the org repo entirely.

## Risks / Trade-offs

**[Risk: .env file committed with real credentials]** → `.env` is already in the default `.gitignore` template. We add it explicitly to our `.gitignore` and ship `.env.example` instead.

**[Risk: env var precedence confusing]** → Document the 4-layer chain clearly in `--help` output and `DEVELOPING.md`. The chain is intuitive: more specific overrides less specific.

**[Risk: clap adds binary size]** → With derive macros, clap adds ~200KB to the release binary. Acceptable for the value it provides.

**[Risk: dotenvy loads unexpected .env in production]** → Only load `.env` in debug builds or when explicitly present. `dotenvy::dotenv().ok()` silently skips if the file doesn't exist, so production deployments without a `.env` file are unaffected.

**[Risk: Display detection is imperfect]** → `$DISPLAY` / `$WAYLAND_DISPLAY` covers Linux. macOS and Windows always have a display. For edge cases, the `open::that()` failure fallback catches the rest. The auth URL is always printed to debug logs regardless.

**[Risk: option_env! and runtime env var name collision]** → Both use `carminedesktop_CLIENT_ID`. At build time, if set, the value is baked in. At runtime, clap reads the live env. Runtime always wins because clap resolves before we check the baked-in constant. Documented clearly in the resolution chain.

**[Risk: build.rs adds complexity]** → The build.rs is ~10 lines and only copies a file if missing. It runs once per fresh clone, then never again (rerun-if-changed prevents re-execution). Minimal overhead.

**[Risk: Private org repo falls behind public releases]** → The org repo pins to a version tag. Updating is a one-line change to the version variable in CI config. No merge conflicts possible since the org repo carries no source code.
