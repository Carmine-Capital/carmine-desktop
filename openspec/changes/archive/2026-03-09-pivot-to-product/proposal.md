## Why

carminedesktop was built with a "branded builds" model — organizations would compile their own customized binary with baked-in credentials and pre-configured mounts. We are pivoting to a multi-tenant product with official releases from the main repo. The branded build infrastructure adds significant complexity (PackagedDefaults, build-time env vars, defaults.toml, org-build docs) that serves a model we are abandoning. carminedesktop will now be the product, and users will configure their own mounts through the wizard.

## What Changes

- **BREAKING**: Remove the entire `PackagedDefaults` system — `PackagedDefaults`, `PackagedMount`, `BrandingConfig`, `TenantConfig`, `DefaultSettings`, `MountOverride` structs; `merge_mounts()`, `has_packaged_config()` functions; `mount_overrides` and `dismissed_packaged_mounts` fields in `UserConfig`
- **BREAKING**: Remove `build/defaults.toml.example` and the `build.rs` file-copy logic
- **BREAKING**: Remove build-time env vars `carminedesktop_CLIENT_ID`, `carminedesktop_TENANT_ID`, `carminedesktop_APP_NAME` (`option_env!()`)
- Hardcode the official carminedesktop client ID (`8ebe3ef7-f509-4146-8fef-c9b5d7c22252`) as a constant
- Expand authentication to support personal Microsoft accounts (MSA) in addition to M365 organizational accounts — the OAuth endpoint already defaults to `common`; the AAD app registration must be configured for `AzureADandPersonalMicrosoftAccount`
- Replace wizard `step-source` + `step-sharepoint` with a unified `step-sources` screen: auto-detects OneDrive, presents SharePoint browser (recent sites + search), allows adding multiple libraries before finishing
- Remove org-build documentation and templates: `docs/builder-guide.md`, `docs/org-build-guide.md`, `docs/templates/`
- Simplify `EffectiveConfig`: remove `tenant_id`, `client_id`, `app_name` fields (no longer sourced from config)
- Simplify `build.rs`: only `tauri_build::build()`, no file copying

## Capabilities

### New Capabilities

None. This is a pivot and cleanup — no new capability specs needed.

### Modified Capabilities

- `packaged-defaults`: **BREAKING** — entire capability removed; spec becomes a tombstone noting the feature no longer exists
- `microsoft-auth`: requirements expand to include personal Microsoft accounts (MSA); `signInAudience` is now `AzureADandPersonalMicrosoftAccount`
- `sharepoint-browser`: wizard flow redesigned — `step-source` + `step-sharepoint` replaced by unified `step-sources`; SharePoint section hidden for personal MSA accounts
- `tray-app`: wizard step sequence updated to reflect `step-sources`; `has_packaged_config()` branch in wizard logic removed
- `config-persistence`: `mount_overrides` and `dismissed_packaged_mounts` removed from user config schema; config resolution simplified (no packaged layer)
- `app-lifecycle`: first-run detection and wizard launch simplified; `packaged` no longer part of app state; OneDrive auto-discovery added to sign-in completion flow

## Impact

- `crates/carminedesktop-core/src/config.rs` — ~200 lines removed
- `crates/carminedesktop-app/src/main.rs` — significant simplification (PackagedDefaults, AppState.packaged, BUILD_* constants, resolve_* functions)
- `crates/carminedesktop-app/build.rs` — remove file-copy logic
- `crates/carminedesktop-app/src/` — wizard HTML/JS: step-sources implementation using existing Graph commands (`get_followed_sites`, `search_sites`, `list_site_drives`)
- `build/defaults.toml.example` — deleted
- `docs/builder-guide.md`, `docs/org-build-guide.md`, `docs/templates/` — deleted
- `docs/azure-ad-setup.md` — reoriented toward contributors
- No changes to `carminedesktop-auth`, `carminedesktop-graph`, `carminedesktop-cache`, or `carminedesktop-vfs`
- Existing user configs (`config.toml`) remain forward-compatible — unknown fields are ignored by serde
