## Why

carminedesktop can be built and tested on all three platforms via CI, but there is no way to produce distributable packages (installers, disk images, AppImages) or publish them as releases. Users currently have no way to install the application without building from source.

## What Changes

- Add a GitHub Actions release workflow (`.github/workflows/release.yml`) that builds platform-specific packages on tag push (`v*`).
- Configure Tauri bundler to produce NSIS installer (Windows), DEB + AppImage (Linux), and DMG (macOS — both aarch64 and x86_64).
- Generate a Tauri updater ed25519 key pair and wire the public key + endpoint URL into `tauri.conf.json` so `tauri-plugin-updater` can verify and fetch updates from GitHub Releases.
- Switch the Windows bundler from WiX/MSI to NSIS (simpler, no external tooling in CI).
- No OS-level code signing in this change — unsigned builds with documentation of the install-unsigned experience per platform.

## Capabilities

### New Capabilities

- `release-pipeline`: CI workflow for building, signing (updater-level), and publishing platform packages to GitHub Releases via `tauri-apps/tauri-action`.

### Modified Capabilities

- `auto-updater`: Set the updater endpoint URL to the repo's GitHub Releases and embed the ed25519 public key, enabling the existing updater logic to function against real releases.

## Impact

- **New file**: `.github/workflows/release.yml`
- **Modified file**: `crates/carminedesktop-app/tauri.conf.json` — updater endpoint, pubkey, switch WiX → NSIS
- **GitHub secrets required**: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- **No code changes** — all application code (updater, bundler config) already exists; this change wires configuration and CI.
- **Existing CI unchanged** — `ci.yml` continues to run on push/PR to main.
