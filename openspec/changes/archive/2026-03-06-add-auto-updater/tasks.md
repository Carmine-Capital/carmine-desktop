## 1. Dependencies

- [x] 1.1 Add `tauri-plugin-updater` to `[workspace.dependencies]` in root `Cargo.toml`
- [x] 1.2 Add `tauri-plugin-updater` to `carminedesktop-app/Cargo.toml` as optional dep behind `desktop` feature
- [x] 1.3 Add `tauri-plugin-process` to workspace and `carminedesktop-app` (needed for `relaunch()` after update)

## 2. Tauri Configuration

- [x] 2.1 Add `plugins.updater` section to `tauri.conf.json` with empty endpoints and empty pubkey (placeholder for branded builds)
- [x] 2.2 Add updater permission to Tauri capabilities (if required by Tauri v2 permission model)

## 3. Update Module

- [x] 3.1 Create `crates/carminedesktop-app/src/update.rs` module (gated behind `#[cfg(feature = "desktop")]`)
- [x] 3.2 Implement `check_for_update()` — uses `UpdaterExt` to check endpoint, returns update info or None
- [x] 3.3 Implement `spawn_update_checker()` — background task: 10s delay → initial check → 4-hour periodic loop, respects cancellation token
- [x] 3.4 Implement `install_and_relaunch()` — triggers graceful shutdown then delegates to updater plugin for install + relaunch
- [x] 3.5 Add shared update state (e.g., `UpdateState` struct with pending update info) accessible from tray module

## 4. Plugin Registration

- [x] 4.1 Register `tauri_plugin_updater::Builder::new().build()` in Tauri builder setup in `main.rs`
- [x] 4.2 Register `tauri_plugin_process::init()` in Tauri builder setup in `main.rs`
- [x] 4.3 Spawn update checker task after mount startup in `setup_after_launch()`

## 5. Tray Menu Integration

- [x] 5.1 Add "Check for Updates" menu item to tray context menu (between "Settings..." and "Sign Out")
- [x] 5.2 Handle "Check for Updates" click — trigger manual update check, notify result
- [x] 5.3 Replace "Check for Updates" with "Restart to Update (v{version})" when an update is pending
- [x] 5.4 Handle "Restart to Update" click — trigger graceful shutdown + update install + relaunch

## 6. Notifications

- [x] 6.1 Send notification when update is downloaded: "{app_name} v{version} is ready — restart to update"
- [x] 6.2 Send notification on manual check when up to date: "{app_name} is up to date"
- [x] 6.3 Send notification on manual check when no endpoint configured: "Update checking is not configured for this build"

## 7. Lifecycle Integration

- [x] 7.1 Cancel update checker task during graceful shutdown (before mount teardown)
- [x] 7.2 Ensure "Restart to Update" runs full graceful shutdown sequence (flush writes, unmount, stop sync) before installing

## 8. Documentation

- [x] 8.1 Update `docs/org-build-guide.md` with updater configuration section (signing key generation, tauri.conf.json patching, workflow changes)
- [x] 8.2 Update `docs/builder-guide.md` with signing key setup for manual builds
- [x] 8.3 Update `docs/templates/github-build.yml` to use `cargo tauri build`, signing env vars, and cross-repo release publishing

## 9. Branded Build Template Updates

- [x] 9.1 Update `docs/templates/github-build.yml` — switch from `cargo build --release` to `cargo tauri build`, add `tauri-cli` installation step
- [x] 9.2 Add signing env vars (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) to workflow template
- [x] 9.3 Add `update.json` generation step to workflow template (build output → update manifest)
- [x] 9.4 Add cross-repo release publishing step (`gh release create --repo`) with fine-grained PAT
- [x] 9.5 Document `tauri.conf.patch.json` pattern for branded builds (productName, identifier, updater endpoint, pubkey)
