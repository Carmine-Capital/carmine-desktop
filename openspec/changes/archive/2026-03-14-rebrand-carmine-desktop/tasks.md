## 1. Workspace & Crate Rename

- [x] 1.1 Rename all 6 crate directories from `cloudmount-*` to `carminedesktop-*`
- [x] 1.2 Update workspace root `Cargo.toml`: package names, dependency paths, member list
- [x] 1.3 Update each crate's `Cargo.toml`: package name, internal dependency references

## 2. Rust Source Rename

- [x] 2.1 Replace all `cloudmount_` / `cloudmount-` references in `use` statements, module paths, and string literals across all `.rs` files
- [x] 2.2 Update env var references from `CLOUDMOUNT_*` to `CARMINEDESKTOP_*` (option_env!, env::var, .env parsing)
- [x] 2.3 Update system integration identifiers: D-Bus names, keyring service names, XDG app IDs
- [x] 2.4 Update user-facing strings: CLI help text, log messages, error messages, notification text
- [x] 2.5 Run `cargo fmt` to fix line-width issues caused by longer identifier names

## 3. Tauri Configuration

- [x] 3.1 Update `tauri.conf.json`: productName → "Carmine Desktop", identifier → "com.carmine-capital.desktop", bundle target → "msi"
- [x] 3.2 Add WiX config with `"language": "en-US"` under `bundle.windows`
- [x] 3.3 Update updater endpoint to `https://static.carminecapital.com/carmine-desktop/latest.json`
- [x] 3.4 Update deep-link scheme to `carminedesktop://` in capabilities JSON files

## 4. Frontend Rebrand

- [x] 4.1 Update HTML files: page titles, app name references
- [x] 4.2 Update JS files: string literals, Tauri IPC references using new crate paths
- [x] 4.3 Update CSS files: class name prefixes if applicable
- [x] 4.4 Ensure no inline event handlers (CSP compliance)

## 5. Auto-Update Infrastructure

- [x] 5.1 Rewrite `release.yml` publish job: replace GitHub Release upload with rsync/SSH to `static.carminecapital.com`
- [x] 5.2 Add `latest.json` generation step in publish job (version, platform URLs, ed25519 signatures)
- [x] 5.3 Document required `DEPLOY_SSH_KEY` GitHub secret

## 6. Windows Installer (NSIS → MSI)

- [x] 6.1 Change bundle target from `"nsis"` to `"msi"` in `tauri.conf.json`
- [x] 6.2 Update WinFsp MSI resource path from `cloudmount-app` to `carminedesktop-app`
- [x] 6.3 Update NSIS hooks.nsh references for transition period

## 7. Build & Documentation

- [x] 7.1 Update Makefile: container name `carminedesktop-build`, crate path references
- [x] 7.2 Update README.md with new project name and structure
- [x] 7.3 Update AGENTS.md with new crate names and conventions
- [x] 7.4 Update `.env.example` with `CARMINEDESKTOP_*` variable names
- [x] 7.5 Update `.gitignore` entries for new crate paths
- [x] 7.6 Update `docs/` references (org-build-guide, azure-ad-setup)

## 8. Verification

- [x] 8.1 Run `cargo fmt --check` — zero formatting issues
- [x] 8.2 Run `cargo clippy --all-targets --all-features` with `-Dwarnings` — zero warnings
- [x] 8.3 Run `cargo build` — successful compilation
- [x] 8.4 Run `cargo test` — all 222 tests pass
- [x] 8.5 Grep for any remaining `cloudmount` / `CloudMount` references (excluding openspec/)
