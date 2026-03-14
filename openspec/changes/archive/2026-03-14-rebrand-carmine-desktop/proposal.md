## Why

The project is being rebranded from "CloudMount" to "Carmine Desktop" to reflect its new identity under the Carmine Capital organization. Alongside the rebrand, the distribution infrastructure is being moved from public GitHub Releases to a private update server (`static.carminecapital.com`), and the Windows installer is being switched from NSIS to MSI (WiX) for better enterprise deployment compatibility.

## What Changes

- **BREAKING**: Full rename from `cloudmount-*` to `carminedesktop-*` across all 6 crate directories, module names, struct prefixes, env var prefixes (`CLOUDMOUNT_*` → `CARMINEDESKTOP_*`), system identifiers (D-Bus, keyring service names), and user-facing strings
- **BREAKING**: Tauri app identity updated — bundle identifier `com.carmine-capital.desktop`, deep-link scheme `carminedesktop://`, product name "Carmine Desktop"
- **BREAKING**: Auto-update endpoint changed from `https://github.com/{owner}/{repo}/releases/latest/download/latest.json` to `https://static.carminecapital.com/carmine-desktop/latest.json`
- **BREAKING**: Release pipeline rewritten — artifacts uploaded via rsync/SSH to private server instead of published as GitHub Releases
- Windows installer format changed from NSIS (`.exe`) to MSI (WiX) for better enterprise/GPO deployment
- Frontend UI rebranded (app name, page titles, CSS class prefixes)
- Build toolbox container reference updated from `carminedesktop-build` to `carminedesktop-build`

## Capabilities

### New Capabilities

_None — this change modifies existing capabilities only._

### Modified Capabilities

- `auto-updater`: Default endpoint changes from GitHub Releases URL to private server URL at `static.carminecapital.com`
- `release-pipeline`: Publishing mechanism changes from GitHub Releases (draft-then-publish) to rsync/SSH upload to private static server; `latest.json` generated locally instead of as a GitHub Release asset
- `developer-experience`: Environment variable prefix changes from `CLOUDMOUNT_*` to `CARMINEDESKTOP_*`; build-time env vars renamed accordingly; `.env.example`, docs, and CLI help text updated
- `winfsp-installer-bundling`: Installer type changes from NSIS to MSI (WiX); WinFsp embedding approach changes accordingly

## Impact

- **All 6 crates**: Directory names, Cargo.toml package names, and all internal references renamed
- **Workspace Cargo.toml**: All dependency paths and package names updated
- **39 Rust source files**: ~757 string/identifier replacements
- **Tauri config** (`tauri.conf.json`, `capabilities/*.json`): App identity, bundle target, updater endpoint
- **Frontend** (`dist/`): HTML titles, JS identifiers, CSS classes
- **CI/CD** (`.github/workflows/release.yml`): Entire publish job rewritten for rsync
- **Build system** (`Makefile`): Container name reference updated
- **Documentation**: README, AGENTS.md, DEVELOPING.md, .env.example all updated
- **New GitHub secret required**: `DEPLOY_SSH_KEY` for rsync to static.carminecapital.com
- **Server-side setup required**: nginx on `static.carminecapital.com` serving `/var/www/static/carmine-desktop/`
