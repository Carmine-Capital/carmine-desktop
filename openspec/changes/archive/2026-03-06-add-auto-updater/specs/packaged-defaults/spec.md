## ADDED Requirements

### Requirement: Updater endpoint configuration for branded builds
The system SHALL support branded build configuration of the auto-updater endpoint and signing public key via `tauri.conf.json` patching.

#### Scenario: Main repo placeholder configuration
- **WHEN** the main repo's `tauri.conf.json` is used without modification (generic/dev build)
- **THEN** the updater plugin is registered but the endpoints list is empty, disabling update checks

#### Scenario: Branded build patches updater endpoint
- **WHEN** a branded build repo patches `tauri.conf.json` before building to set `plugins.updater.endpoints` and `plugins.updater.pubkey`
- **THEN** the built binary checks the specified endpoint for updates and verifies signatures against the specified public key

#### Scenario: Branded build patches product identity
- **WHEN** a branded build repo patches `tauri.conf.json` to set `productName` and `identifier`
- **THEN** the built installers use the branded product name and identifier for the installed application

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
