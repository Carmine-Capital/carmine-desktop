## MODIFIED Requirements

### Requirement: Two-layer configuration merge
The system SHALL resolve the effective configuration by merging four layers: CLI/env overrides, user config, and packaged defaults, where higher layers take precedence.

#### Scenario: Setting from CLI/env override
- **WHEN** a setting is provided via CLI argument or environment variable
- **THEN** the effective value is the CLI/env value, overriding both user config and packaged defaults

#### Scenario: Setting exists only in packaged defaults
- **WHEN** a setting has a value in packaged defaults but the user has not set it and no CLI/env override exists
- **THEN** the effective value is the packaged default

#### Scenario: Setting exists in both layers
- **WHEN** a setting has a value in packaged defaults and the user has explicitly set a different value, with no CLI/env override
- **THEN** the effective value is the user's value

#### Scenario: Setting exists only in user config
- **WHEN** a setting has a value in user config but not in packaged defaults (e.g., a user-added mount)
- **THEN** the effective value is the user's value

#### Scenario: Mount union merge
- **WHEN** both packaged defaults and user config contain mount definitions
- **THEN** the effective mount list is the union: all packaged mounts plus all user-added mounts. If a user mount has the same `id` as a packaged mount, the user's values for that mount override the packaged values field by field.

#### Scenario: Full precedence chain for client_id
- **WHEN** the system resolves client_id
- **THEN** it checks in order: (1) `--client-id` CLI arg, (2) `carminedesktop_CLIENT_ID` runtime env var, (3) `carminedesktop_CLIENT_ID` build-time `option_env!()`, (4) `packaged_defaults.tenant.client_id` from `defaults.toml`, (5) built-in `DEFAULT_CLIENT_ID` constant

#### Scenario: Full precedence chain for tenant_id
- **WHEN** the system resolves tenant_id
- **THEN** it checks in order: (1) `--tenant-id` CLI arg, (2) `carminedesktop_TENANT_ID` runtime env var, (3) `carminedesktop_TENANT_ID` build-time `option_env!()`, (4) `packaged_defaults.tenant.id` from `defaults.toml`, (5) None (common endpoint used)

## ADDED Requirements

### Requirement: Build-time defaults file template
The repository SHALL track `build/defaults.toml.example` as a documented template and gitignore `build/defaults.toml`. A `build.rs` script in `carminedesktop-app` SHALL copy the template to `defaults.toml` if the file is missing, ensuring compilation succeeds on fresh clones.

#### Scenario: Fresh clone compilation
- **WHEN** a developer clones the repository and `build/defaults.toml` does not exist
- **THEN** `build.rs` copies `build/defaults.toml.example` to `build/defaults.toml` before `include_str!()` is evaluated, and compilation succeeds with generic defaults

#### Scenario: Custom defaults.toml preserved
- **WHEN** `build/defaults.toml` already exists (placed by CI or manually)
- **THEN** `build.rs` does not overwrite it, preserving the org-specific mount definitions and branding

#### Scenario: Rebuild after defaults.toml change
- **WHEN** the contents of `build/defaults.toml` change
- **THEN** `build.rs` triggers a recompile of `carminedesktop-app` via `cargo::rerun-if-changed`
