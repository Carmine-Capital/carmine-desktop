## ADDED Requirements

### Requirement: CLI argument parsing
The system SHALL parse command-line arguments using `clap` with derive macros, providing `--help`, `--version`, and runtime configuration overrides.

#### Scenario: Help output
- **WHEN** the user runs the application with `--help`
- **THEN** the system prints usage information listing all available options and exits with code 0

#### Scenario: Version output
- **WHEN** the user runs the application with `--version`
- **THEN** the system prints the application name and version (from Cargo.toml) and exits with code 0

#### Scenario: Client ID override via CLI
- **WHEN** the user runs the application with `--client-id <uuid>`
- **THEN** the system uses the provided client ID for authentication, overriding all other sources (env var, user config, packaged defaults)

#### Scenario: Tenant ID override via CLI
- **WHEN** the user runs the application with `--tenant-id <id>`
- **THEN** the system uses the provided tenant ID for authentication, overriding all other sources

#### Scenario: Custom config file path
- **WHEN** the user runs the application with `--config <path>`
- **THEN** the system loads user configuration from the specified path instead of the platform default location

#### Scenario: Log level override via CLI
- **WHEN** the user runs the application with `--log-level debug`
- **THEN** the system sets the tracing subscriber filter to the specified level, overriding the default and RUST_LOG

#### Scenario: Headless flag with desktop feature
- **WHEN** the user runs the application with `--headless` and the binary was compiled with the `desktop` feature
- **THEN** the system runs in headless mode (no Tauri GUI) despite the desktop feature being available

### Requirement: Environment variable configuration
The system SHALL support `CLOUDMOUNT_*` environment variables as a configuration override layer between CLI arguments and the user config file.

#### Scenario: Client ID from environment
- **WHEN** `CLOUDMOUNT_CLIENT_ID` is set and no `--client-id` CLI argument is provided
- **THEN** the system uses the environment variable value for authentication

#### Scenario: Tenant ID from environment
- **WHEN** `CLOUDMOUNT_TENANT_ID` is set and no `--tenant-id` CLI argument is provided
- **THEN** the system uses the environment variable value for authentication

#### Scenario: Log level from environment
- **WHEN** `CLOUDMOUNT_LOG_LEVEL` is set and no `--log-level` CLI argument is provided
- **THEN** the system uses the environment variable value for the tracing filter

#### Scenario: Config path from environment
- **WHEN** `CLOUDMOUNT_CONFIG` is set and no `--config` CLI argument is provided
- **THEN** the system loads user configuration from the path specified in the environment variable

#### Scenario: CLI overrides environment
- **WHEN** both `--client-id` CLI argument and `CLOUDMOUNT_CLIENT_ID` environment variable are set
- **THEN** the CLI argument value takes precedence

### Requirement: .env file support
The system SHALL load environment variables from a `.env` file in the current working directory before parsing CLI arguments.

#### Scenario: .env file present
- **WHEN** a `.env` file exists in the current working directory
- **THEN** the system loads its key=value pairs as environment variables before CLI/env parsing, so they are available as fallback values

#### Scenario: .env file absent
- **WHEN** no `.env` file exists in the current working directory
- **THEN** the system continues normally without error

#### Scenario: .env does not override explicit env vars
- **WHEN** both a `.env` file and an explicit shell environment variable define the same key
- **THEN** the explicit shell environment variable takes precedence (dotenvy default behavior)

### Requirement: Startup pre-flight validation
The system SHALL perform validation checks after configuration resolution but before component initialization, exiting early with actionable error messages when critical problems are detected.

#### Scenario: Placeholder client ID detected
- **WHEN** the resolved client ID equals the placeholder value `00000000-0000-0000-0000-000000000000`
- **THEN** the system prints an error message to stderr explaining that a valid Azure AD client ID is required, references `docs/azure-ad-setup.md` for setup instructions, mentions the `--client-id` flag and `CLOUDMOUNT_CLIENT_ID` env var as alternatives, and exits with code 1 without attempting authentication

#### Scenario: FUSE not available on Linux
- **WHEN** the system is running on Linux and `fusermount3` is not found in PATH
- **THEN** the system logs a warning "FUSE not available — install libfuse3-dev to enable filesystem mounts" and continues startup (mounts will fail individually when attempted)

#### Scenario: FUSE not available on macOS
- **WHEN** the system is running on macOS and `fusermount` is not found in PATH
- **THEN** the system logs a warning "FUSE not available — install macFUSE to enable filesystem mounts" and continues startup

#### Scenario: All pre-flight checks pass
- **WHEN** the client ID is valid and system dependencies are available
- **THEN** the system proceeds with normal component initialization

### Requirement: .env.example template
The repository SHALL include a `.env.example` file documenting all supported environment variables with placeholder values and comments.

#### Scenario: Developer copies template
- **WHEN** a developer copies `.env.example` to `.env` and fills in their Azure AD credentials
- **THEN** the application loads the credentials from `.env` on next run without requiring recompilation

### Requirement: Build-time configuration injection
The build system SHALL support injecting `client_id`, `tenant_id`, and `app_name` at compile time via environment variables, so CI pipelines can produce branded binaries without managing configuration files.

#### Scenario: CI sets client ID at build time
- **WHEN** the environment variable `CLOUDMOUNT_CLIENT_ID` is set during `cargo build`
- **THEN** the value is baked into the binary via `option_env!()` and used as a fallback when no runtime CLI arg or env var provides a client_id

#### Scenario: CI sets tenant ID at build time
- **WHEN** the environment variable `CLOUDMOUNT_TENANT_ID` is set during `cargo build`
- **THEN** the value is baked into the binary and used as a fallback for tenant_id resolution

#### Scenario: CI sets app name at build time
- **WHEN** the environment variable `CLOUDMOUNT_APP_NAME` is set during `cargo build`
- **THEN** the value is baked into the binary and used as the application display name (tray tooltip, window titles, notifications)

#### Scenario: No build-time env vars set
- **WHEN** no `CLOUDMOUNT_*` environment variables are set during `cargo build`
- **THEN** the build succeeds and the binary falls back to `defaults.toml` values or hardcoded defaults

#### Scenario: Runtime override takes precedence over build-time
- **WHEN** a value was baked in at build time via `option_env!()` AND the same value is provided at runtime via CLI arg or env var
- **THEN** the runtime value takes precedence

### Requirement: defaults.toml template pattern
The repository SHALL track `build/defaults.toml.example` as a template and gitignore `build/defaults.toml`. A `build.rs` script SHALL auto-copy the template to `defaults.toml` if missing, ensuring fresh clones compile without manual steps.

#### Scenario: Fresh clone builds without manual steps
- **WHEN** a developer clones the repository and runs `cargo build`
- **THEN** `build.rs` detects that `build/defaults.toml` does not exist, copies `build/defaults.toml.example` to `build/defaults.toml`, and the build succeeds with empty/generic defaults

#### Scenario: Org builder provides custom defaults.toml
- **WHEN** a builder places a custom `build/defaults.toml` with SharePoint mount definitions before running `cargo build`
- **THEN** `build.rs` detects the file already exists and does not overwrite it; the custom values are embedded into the binary

#### Scenario: defaults.toml not accidentally committed
- **WHEN** a developer runs `git status` after creating a custom `build/defaults.toml`
- **THEN** the file does not appear as untracked or modified because it is listed in `.gitignore`

### Requirement: Org build guide
The repository SHALL include `docs/org-build-guide.md` with instructions for setting up a private config overlay repo that builds org-branded CloudMount binaries.

#### Scenario: Setting up a GitLab org build
- **WHEN** an org admin reads `docs/org-build-guide.md`
- **THEN** they find step-by-step instructions for: creating a private GitLab repo with `defaults.toml` and `.gitlab-ci.yml`, configuring CI variables (CLIENT_ID masked, TENANT_ID, APP_NAME), and the CI pipeline that clones the public repo at a pinned version, injects config, and builds

#### Scenario: Setting up a GitHub org build
- **WHEN** an org admin reads `docs/org-build-guide.md`
- **THEN** they find equivalent instructions using a private GitHub repo with `.github/workflows/build.yml` and GitHub Secrets/Variables

#### Scenario: Updating to a new CloudMount version
- **WHEN** the org admin wants to update to a new CloudMount release
- **THEN** they change the version tag variable in their CI config (one-line change) and trigger a new build; no merge conflicts or source code changes needed

### Requirement: Developer documentation
The repository SHALL include a `DEVELOPING.md` file with setup instructions for developers testing the application.

#### Scenario: Developer reads setup guide
- **WHEN** a developer opens `DEVELOPING.md`
- **THEN** they find: system dependency requirements per platform (libfuse3-dev, GTK4, macFUSE), Azure AD app registration steps (or link to docs/azure-ad-setup.md), how to provide credentials (.env, env vars, CLI args), build commands for headless and desktop modes, what to expect on first run, and a reference to `docs/org-build-guide.md` for org-branded builds
