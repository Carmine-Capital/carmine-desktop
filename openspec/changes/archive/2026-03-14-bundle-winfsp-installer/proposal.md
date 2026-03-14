## Why

Users must manually install WinFsp before using CloudMount on Windows. This creates a poor first-run experience — the app fails with a cryptic DLL error or a preflight dialog directing users to an external download. Bundling WinFsp with the NSIS installer eliminates this friction and matches what other WinFsp-based apps (Cryptomator, Parsec) do.

## What Changes

- The NSIS installer embeds the WinFsp MSI and silently installs it if WinFsp is not already present on the system
- CI workflows download a pinned WinFsp MSI from GitHub releases during the Windows build
- Tauri NSIS configuration adds an installer hook for the pre-install phase
- WinFsp copyright attribution is added to the settings page (redistribution license requirement)
- WinFsp is **not** removed on CloudMount uninstall (other apps may depend on it)
- The existing runtime preflight check in `main.rs` is preserved as a safety net for manual/portable installs

## Capabilities

### New Capabilities
- `winfsp-installer-bundling`: Embedding and silent installation of WinFsp MSI via NSIS pre-install hook

### Modified Capabilities
- `release-pipeline`: CI workflows gain a step to download the pinned WinFsp MSI for bundling into the NSIS installer
- `platform-preflight`: No spec-level changes — existing behavior preserved as fallback

## Impact

- **CI**: `release.yml` and `build-installer.yml` gain a WinFsp MSI download step (Windows jobs only)
- **Installer size**: Increases by ~1.5 MB (WinFsp MSI)
- **New files**: `crates/cloudmount-app/windows/hooks.nsh`, updated `tauri.conf.json`
- **Dependencies**: WinFsp MSI version pinned in CI — must be updated when upgrading the `winfsp` crate
- **Legal**: WinFsp GPLv3 redistribution exception applies (CloudMount is MIT); requires copyright notice in UI
