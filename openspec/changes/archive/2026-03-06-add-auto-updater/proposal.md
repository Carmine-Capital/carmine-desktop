## Why

CloudMount branded builds (like Carmine Drive) need to deliver updates to ~40 users without manual intervention. Currently the build pipeline produces raw binaries with no installer packaging and no update mechanism. Users would need to manually download and replace binaries for every release. Adding auto-update support is a prerequisite for production deployment.

## What Changes

- Add `tauri-plugin-updater` dependency to `cloudmount-app`
- Configure the Tauri updater in `tauri.conf.json` with a placeholder endpoint URL (branded builds override this)
- Register the updater plugin in the Tauri builder setup
- Add update check logic: check on launch + periodic checks
- Add update-related UI: tray menu "Check for Updates" item, notification when update is available
- Support updater endpoint override via `tauri.conf.json` patching (branded build pattern)
- Upgrade branded build workflow from `cargo build --release` to `cargo tauri build` to produce platform installers (`.deb`, `.AppImage`, `.dmg`, `.msi`) with updater signatures
- Document the signing key setup and branded build updater configuration

## Capabilities

### New Capabilities
- `auto-updater`: Automatic update checking, downloading, signature verification, and installation via Tauri's updater plugin. Covers update lifecycle, endpoint configuration, signing, and user-facing update UI.

### Modified Capabilities
- `tray-app`: Add "Check for Updates" menu item and update-available notification
- `app-lifecycle`: Add updater plugin initialization during startup, handle update-then-restart flow
- `packaged-defaults`: Document updater endpoint URL as a branded build configuration point; extend `tauri.conf.json` patching pattern for updater settings

## Impact

- **Dependencies**: `tauri-plugin-updater` added to workspace and `cloudmount-app`
- **Code**: `main.rs` (plugin registration), `tray.rs` (menu item), new update check module
- **Config**: `tauri.conf.json` gains `plugins.updater` section
- **Build pipeline**: Branded repos switch from `cargo build` to `cargo tauri build`, add signing key management
- **Branded repos**: Need `tauri.conf.patch.json` for updater endpoint + signing public key, workflow changes for `cargo tauri build` + cross-repo release publishing
- **Infrastructure**: Each branded build uses a public GitHub release repo for hosting installers and `update.json` manifests
