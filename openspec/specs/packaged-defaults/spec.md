### Requirement: Build-time defaults file
The system SHALL support an optional `build/defaults.toml` file that is embedded into the binary at compile time via `include_str!`. This file defines the packaged configuration layer.

#### Scenario: Build with defaults present
- **WHEN** the builder runs `cargo tauri build` and `build/defaults.toml` exists in the project root
- **THEN** the binary contains the full contents of the file as an embedded string constant, and the build succeeds

#### Scenario: Build without defaults
- **WHEN** the builder runs `cargo tauri build` and `build/defaults.toml` does not exist or is empty
- **THEN** the binary contains no packaged defaults, the application behaves as a generic self-service build, and the full first-run wizard is presented

#### Scenario: Defaults file format
- **WHEN** the builder creates `build/defaults.toml`
- **THEN** the file supports these sections: `[tenant]` (id, client_id), `[branding]` (app_name), `[defaults]` (auto_start, cache_max_size, sync_interval_secs), and `[[mounts]]` (id, name, type, mount_point, and type-specific fields: drive_id, site_id, library_name)

### Requirement: Packaged mount definitions
The system SHALL support pre-configured mount definitions in `build/defaults.toml` that are automatically activated on first run.

#### Scenario: Packaged OneDrive mount
- **WHEN** `build/defaults.toml` contains a `[[mounts]]` entry with `type = "onedrive"`
- **THEN** after the user signs in for the first time, the system automatically mounts the user's OneDrive at the specified mount point without requiring site selection

#### Scenario: Packaged SharePoint mount
- **WHEN** `build/defaults.toml` contains a `[[mounts]]` entry with `type = "sharepoint"`, `site_id`, `drive_id`, and `library_name`
- **THEN** after the user signs in for the first time, the system automatically mounts the specified SharePoint document library at the specified mount point without requiring site browsing

#### Scenario: Mount point template expansion
- **WHEN** a packaged mount's `mount_point` contains `{home}`
- **THEN** the system expands `{home}` to the user's home directory at runtime (e.g., `/home/alice`, `C:\Users\Alice`, `/Users/alice`)

#### Scenario: Stable mount IDs
- **WHEN** packaged mounts are defined
- **THEN** each mount MUST have a unique `id` field (string) that remains stable across application versions to enable tracking across updates

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
- **THEN** it checks in order: (1) `--client-id` CLI arg, (2) `CLOUDMOUNT_CLIENT_ID` runtime env var, (3) `CLOUDMOUNT_CLIENT_ID` build-time `option_env!()`, (4) `packaged_defaults.tenant.client_id` from `defaults.toml`, (5) built-in `DEFAULT_CLIENT_ID` constant

#### Scenario: Full precedence chain for tenant_id
- **WHEN** the system resolves tenant_id
- **THEN** it checks in order: (1) `--tenant-id` CLI arg, (2) `CLOUDMOUNT_TENANT_ID` runtime env var, (3) `CLOUDMOUNT_TENANT_ID` build-time `option_env!()`, (4) `packaged_defaults.tenant.id` from `defaults.toml`, (5) None (common endpoint used)

### Requirement: Update behavior for packaged defaults
The system SHALL automatically apply new packaged defaults from updated app versions without overwriting user changes.

#### Scenario: Packaged default updated, user never changed it
- **WHEN** the app is updated and a packaged default value changes (e.g., a SharePoint site_id migrated to a new URL), and the user never overrode that value
- **THEN** the new packaged value takes effect immediately after the update

#### Scenario: Packaged default updated, user has an override
- **WHEN** the app is updated and a packaged default value changes, but the user has explicitly set a different value for that setting
- **THEN** the user's override is preserved; the new packaged value is ignored for that setting

#### Scenario: New packaged mount added in update
- **WHEN** the app is updated and the new version's packaged defaults contain a mount ID that did not exist in the previous version
- **THEN** the new packaged mount appears automatically after the update (user is notified via notification)

#### Scenario: Packaged mount removed in update
- **WHEN** the app is updated and a previously packaged mount ID is no longer in the new version's defaults
- **THEN** the mount is removed from the effective config unless the user has made modifications to it, in which case it becomes a user-owned mount

### Requirement: User can dismiss packaged mounts
The system SHALL allow users to dismiss (hide) pre-configured mounts they do not want.

#### Scenario: User dismisses a packaged mount
- **WHEN** the user removes a packaged mount from the settings UI
- **THEN** the system records the mount ID as dismissed in user config, and the mount no longer appears in the effective mount list

#### Scenario: Dismissed mount reappears after significant change
- **WHEN** the user has dismissed a packaged mount, and a new app version changes that mount's `id`
- **THEN** the mount is treated as a new mount and appears again (since the old dismissal was for a different ID)

#### Scenario: User un-dismisses a mount
- **WHEN** the user wants to restore a dismissed packaged mount
- **THEN** the settings UI provides a "Restore default mounts" option that clears all dismissals

### Requirement: Reset to packaged defaults
The system SHALL allow users to reset individual settings or all settings back to packaged defaults.

#### Scenario: Reset single setting
- **WHEN** the user clicks "Reset to Default" for a specific setting in the settings UI
- **THEN** the user's override for that setting is removed from user config, and the effective value reverts to the packaged default

#### Scenario: Reset all settings
- **WHEN** the user clicks "Reset All to Defaults" in the settings UI
- **THEN** the user config is cleared entirely (except authentication tokens), and all effective values revert to packaged defaults. User-added mounts are removed after confirmation.

### Requirement: Packaged tenant and branding
The system SHALL use the packaged tenant and branding information to customize the authentication flow and UI.

#### Scenario: Pre-configured tenant ID
- **WHEN** `build/defaults.toml` contains a `[tenant]` section with `id`
- **THEN** the OAuth2 authorization URL includes `&domain_hint={tenant_id}` so the Microsoft login page skips organization selection and goes directly to the correct tenant login

#### Scenario: Pre-configured client ID
- **WHEN** `build/defaults.toml` contains a `[tenant]` section with `client_id`
- **THEN** the OAuth2 flow uses this client ID instead of the generic CloudMount app registration

#### Scenario: Custom app name
- **WHEN** `build/defaults.toml` contains `[branding]` with `app_name`
- **THEN** the system tray tooltip, window titles, notification titles, and wizard header all display the custom app name instead of "CloudMount"

#### Scenario: No branding configured
- **WHEN** `build/defaults.toml` does not contain a `[branding]` section
- **THEN** the application uses "CloudMount" as the default app name everywhere

### Requirement: Build-time defaults file template
The repository SHALL track `build/defaults.toml.example` as a documented template and gitignore `build/defaults.toml`. A `build.rs` script in `cloudmount-app` SHALL copy the template to `defaults.toml` if the file is missing, ensuring compilation succeeds on fresh clones.

#### Scenario: Fresh clone compilation
- **WHEN** a developer clones the repository and `build/defaults.toml` does not exist
- **THEN** `build.rs` copies `build/defaults.toml.example` to `build/defaults.toml` before `include_str!()` is evaluated, and compilation succeeds with generic defaults

#### Scenario: Custom defaults.toml preserved
- **WHEN** `build/defaults.toml` already exists (placed by CI or manually)
- **THEN** `build.rs` does not overwrite it, preserving the org-specific mount definitions and branding

#### Scenario: Rebuild after defaults.toml change
- **WHEN** the contents of `build/defaults.toml` change
- **THEN** `build.rs` triggers a recompile of `cloudmount-app` via `cargo::rerun-if-changed`
