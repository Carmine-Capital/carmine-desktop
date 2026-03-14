## MODIFIED Requirements

### Requirement: Environment variable configuration
The system SHALL support `CARMINEDESKTOP_*` environment variables as a configuration override layer between CLI arguments and the user config file.

#### Scenario: Client ID from environment
- **WHEN** `CARMINEDESKTOP_CLIENT_ID` is set and no `--client-id` CLI argument is provided
- **THEN** the system uses the environment variable value for authentication

#### Scenario: Tenant ID from environment
- **WHEN** `CARMINEDESKTOP_TENANT_ID` is set and no `--tenant-id` CLI argument is provided
- **THEN** the system uses the environment variable value for authentication

#### Scenario: Log level from environment
- **WHEN** `CARMINEDESKTOP_LOG_LEVEL` is set and no `--log-level` CLI argument is provided
- **THEN** the system uses the environment variable value for the tracing filter

#### Scenario: Config path from environment
- **WHEN** `CARMINEDESKTOP_CONFIG` is set and no `--config` CLI argument is provided
- **THEN** the system loads user configuration from the path specified in the environment variable

#### Scenario: CLI overrides environment
- **WHEN** both `--client-id` CLI argument and `CARMINEDESKTOP_CLIENT_ID` environment variable are set
- **THEN** the CLI argument value takes precedence

### Requirement: Build-time configuration injection
The build system SHALL support injecting `client_id`, `tenant_id`, and `app_name` at compile time via environment variables, so CI pipelines can produce branded binaries without managing configuration files.

#### Scenario: CI sets client ID at build time
- **WHEN** the environment variable `CARMINEDESKTOP_CLIENT_ID` is set during `cargo build`
- **THEN** the value is baked into the binary via `option_env!()` and used as a fallback when no runtime CLI arg or env var provides a client_id

#### Scenario: CI sets tenant ID at build time
- **WHEN** the environment variable `CARMINEDESKTOP_TENANT_ID` is set during `cargo build`
- **THEN** the value is baked into the binary and used as a fallback for tenant_id resolution

#### Scenario: CI sets app name at build time
- **WHEN** the environment variable `CARMINEDESKTOP_APP_NAME` is set during `cargo build`
- **THEN** the value is baked into the binary and used as the application display name (tray tooltip, window titles, notifications)

#### Scenario: No build-time env vars set
- **WHEN** no `CARMINEDESKTOP_*` environment variables are set during `cargo build`
- **THEN** the build succeeds and the binary falls back to `defaults.toml` values or hardcoded defaults

#### Scenario: Runtime override takes precedence over build-time
- **WHEN** a value was baked in at build time via `option_env!()` AND the same value is provided at runtime via CLI arg or env var
- **THEN** the runtime value takes precedence

### Requirement: Startup pre-flight validation
The system SHALL perform validation checks after configuration resolution but before component initialization, exiting early with actionable error messages when critical problems are detected.

#### Scenario: Placeholder client ID detected
- **WHEN** the resolved client ID equals the placeholder value `00000000-0000-0000-0000-000000000000`
- **THEN** the system prints an error message to stderr explaining that a valid Azure AD client ID is required, references `docs/azure-ad-setup.md` for setup instructions, mentions the `--client-id` flag and `CARMINEDESKTOP_CLIENT_ID` env var as alternatives, and exits with code 1 without attempting authentication
