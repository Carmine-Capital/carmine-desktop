## REMOVED Requirements

### Requirement: Build-time defaults file
**Reason**: Branded build model abandoned. CloudMount is now a multi-tenant product with official releases; embedding org-specific credentials and mounts at compile time is no longer supported.
**Migration**: No end-user migration required. Config files with unknown fields (e.g., `mount_overrides`, `dismissed_packaged_mounts`) are silently ignored by serde.

### Requirement: Updater endpoint configuration for branded builds
**Reason**: Branded builds are no longer supported. CloudMount ships a single official binary with its own update channel.
**Migration**: N/A.

### Requirement: Packaged mount definitions
**Reason**: Pre-configured mounts are replaced by user self-service via the wizard's `step-sources` screen.
**Migration**: N/A.

### Requirement: Two-layer configuration merge
**Reason**: Without packaged defaults, configuration is a single user layer merged with built-in defaults. The merge logic and `PackagedDefaults` struct are removed entirely.
**Migration**: N/A. `EffectiveConfig::build()` no longer accepts a `PackagedDefaults` parameter.

### Requirement: Update behavior for packaged defaults
**Reason**: No packaged defaults to update.
**Migration**: N/A.

### Requirement: User can dismiss packaged mounts
**Reason**: No packaged mounts to dismiss. `dismissed_packaged_mounts` field removed from `UserConfig`.
**Migration**: Existing config files with this field are forward-compatible; serde silently drops unknown fields.

### Requirement: Reset to packaged defaults
**Reason**: No packaged defaults to reset to. "Reset to Default" reverts to built-in application defaults only.
**Migration**: N/A.

### Requirement: Packaged tenant and branding
**Reason**: The OAuth2 flow always uses the official CloudMount client ID and the `common` endpoint. Branding is fixed as "CloudMount".
**Migration**: N/A.

### Requirement: Build-time defaults file template
**Reason**: `build/defaults.toml.example` and the `build.rs` file-copy logic are removed. `build.rs` only runs `tauri_build::build()`.
**Migration**: N/A.
