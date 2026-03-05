## 1. Rename crate directories

- [x] 1.1 `git mv crates/filesync-core crates/cloudmount-core`
- [x] 1.2 `git mv crates/filesync-auth crates/cloudmount-auth`
- [x] 1.3 `git mv crates/filesync-graph crates/cloudmount-graph`
- [x] 1.4 `git mv crates/filesync-cache crates/cloudmount-cache`
- [x] 1.5 `git mv crates/filesync-vfs crates/cloudmount-vfs`
- [x] 1.6 `git mv crates/filesync-app crates/cloudmount-app`

## 2. Update Cargo.toml files

- [x] 2.1 Update root `Cargo.toml`: rename all workspace members from `crates/filesync-*` to `crates/cloudmount-*` and all `[workspace.dependencies]` entries from `filesync-*` to `cloudmount-*` with updated paths
- [x] 2.2 Update `crates/cloudmount-core/Cargo.toml`: rename package to `cloudmount-core`
- [x] 2.3 Update `crates/cloudmount-auth/Cargo.toml`: rename package to `cloudmount-auth`, update dependency `filesync-core` → `cloudmount-core`
- [x] 2.4 Update `crates/cloudmount-graph/Cargo.toml`: rename package to `cloudmount-graph`, update dependency `filesync-core` → `cloudmount-core`
- [x] 2.5 Update `crates/cloudmount-cache/Cargo.toml`: rename package to `cloudmount-cache`, update dependencies `filesync-core` → `cloudmount-core`, `filesync-graph` → `cloudmount-graph`
- [x] 2.6 Update `crates/cloudmount-vfs/Cargo.toml`: rename package to `cloudmount-vfs`, update dependencies `filesync-core` → `cloudmount-core`, `filesync-graph` → `cloudmount-graph`, `filesync-cache` → `cloudmount-cache`
- [x] 2.7 Update `crates/cloudmount-app/Cargo.toml`: rename package to `cloudmount-app`, update all 5 dependencies from `filesync-*` → `cloudmount-*`

## 3. Update Rust source — module paths and imports

- [x] 3.1 Replace all `use filesync_core` with `use cloudmount_core` across all `.rs` files (~15 files)
- [x] 3.2 Replace all `use filesync_auth` with `use cloudmount_auth` across all `.rs` files (~3 files)
- [x] 3.3 Replace all `use filesync_graph` with `use cloudmount_graph` across all `.rs` files (~3 files)
- [x] 3.4 Replace all `use filesync_cache` with `use cloudmount_cache` across all `.rs` files (~5 files)
- [x] 3.5 Replace all `use filesync_vfs` with `use cloudmount_vfs` across all `.rs` files (~3 files)
- [x] 3.6 Replace all `filesync_core::` qualified paths (error types, Result types) with `cloudmount_core::` across all `.rs` files
- [x] 3.7 Replace all `filesync_cache::` and `filesync_vfs::` qualified paths with `cloudmount_cache::` and `cloudmount_vfs::` equivalents

## 4. Update Rust source — constants and string literals

- [x] 4.1 In `cloudmount-core/src/config.rs`: change `DEFAULT_APP_NAME` from `"FileSync"` to `"CloudMount"`
- [x] 4.2 In `cloudmount-core/src/config.rs`: change config dir join from `"filesync"` to `"cloudmount"` in `config_dir()` and `cache_dir()` functions
- [x] 4.3 In `cloudmount-core/src/config.rs`: change systemd service name from `"filesync.service"` to `"cloudmount.service"` and description from `"FileSync"` to `"CloudMount"` in the `enable()` function (Linux)
- [x] 4.4 In `cloudmount-core/src/config.rs`: change macOS LaunchAgent identifier from `"com.filesync.agent"` to `"com.cloudmount.agent"` and plist filename from `"com.filesync.agent.plist"` to `"com.cloudmount.agent.plist"`
- [x] 4.5 In `cloudmount-core/src/config.rs`: change Windows registry value name from `"FileSync"` to `"CloudMount"`
- [x] 4.6 In `cloudmount-auth/src/storage.rs`: change `SERVICE_NAME` from `"filesync"` to `"cloudmount"`
- [x] 4.7 In `cloudmount-auth/src/storage.rs`: change encrypted token directory join from `"filesync"` to `"cloudmount"` and fallback password prefix from `"filesync-fallback-"` to `"cloudmount-fallback-"`
- [x] 4.8 In `cloudmount-vfs/src/fuse_fs.rs`: change FUSE `FSName` from `"filesync"` to `"cloudmount"`
- [x] 4.9 In `cloudmount-vfs/src/cfapi.rs`: change `PROVIDER_NAME` from `"FileSync"` to `"CloudMount"`
- [x] 4.10 In `cloudmount-app/src/main.rs`: change SQLite database filename from `"filesync.db"` to `"cloudmount.db"`
- [x] 4.11 In `cloudmount-app/src/tray.rs`: change tray icon ID from `"filesync-tray"` to `"cloudmount-tray"`

## 5. Update Rust source — struct names

- [x] 5.1 In `cloudmount-vfs/src/fuse_fs.rs`: rename struct `FileSyncFs` to `CloudMountFs` and update all references
- [x] 5.2 In `cloudmount-vfs/src/cfapi.rs`: rename struct `FileSyncCfFilter` to `CloudMountCfFilter` and update all references
- [x] 5.3 Update any references to `FileSyncFs` or `FileSyncCfFilter` in other files (mount.rs, lib.rs, tests)

## 6. Update test files

- [x] 6.1 Update `cloudmount-core/tests/config_tests.rs`: replace all `filesync_core` module paths with `cloudmount_core`
- [x] 6.2 Update `cloudmount-auth/tests/auth_integration.rs`: replace all `filesync_auth` module paths with `cloudmount_auth`
- [x] 6.3 Update `cloudmount-graph/tests/graph_tests.rs`: replace all `filesync_graph` module paths with `cloudmount_graph`
- [x] 6.4 Update `cloudmount-cache/tests/cache_tests.rs`: replace all `filesync_cache` module paths with `cloudmount_cache`
- [x] 6.5 Update `cloudmount-vfs/tests/fuse_integration.rs`: replace all `filesync_vfs` module paths with `cloudmount_vfs`
- [x] 6.6 Update `cloudmount-vfs/tests/cfapi_integration.rs`: replace all `filesync_vfs` module paths with `cloudmount_vfs`
- [x] 6.7 Update `cloudmount-app/tests/integration_tests.rs`: replace all `filesync_*` module paths with `cloudmount_*` and `"filesync.db"` with `"cloudmount.db"`

## 7. Update Tauri configuration and HTML templates

- [x] 7.1 In `cloudmount-app/tauri.conf.json`: change `productName` from `"FileSync"` to `"CloudMount"` and `identifier` from `"com.filesync.app"` to `"com.cloudmount.app"`
- [x] 7.2 In `cloudmount-app/dist/wizard.html`: change `<title>` from `"FileSync Setup"` to `"CloudMount Setup"` and app title element from `"FileSync"` to `"CloudMount"`
- [x] 7.3 In `cloudmount-app/dist/settings.html`: change `<title>` from `"FileSync Settings"` to `"CloudMount Settings"`

## 8. Update CI/CD workflows

- [x] 8.1 In `.github/workflows/ci.yml`: replace `filesync-vfs` references with `cloudmount-vfs`
- [x] 8.2 In `.github/workflows/build-installer.yml`: change default product name from `"FileSync"` to `"CloudMount"` and working directory from `crates/filesync-app` to `crates/cloudmount-app`

## 9. Update documentation

- [x] 9.1 Update `README.md`: replace all "FileSync" with "CloudMount", update `cargo` command examples from `filesync-app` to `cloudmount-app`, update config path examples from `filesync` to `cloudmount`
- [x] 9.2 Update `docs/azure-ad-setup.md`: replace "FileSync" references with "CloudMount"
- [x] 9.3 Update `docs/builder-guide.md`: replace "FileSync" references with "CloudMount"
- [x] 9.4 Update `build/defaults.toml`: change comment header from "FileSync" to "CloudMount" and update example branding references

## 10. Update OpenSpec main specifications

- [x] 10.1 Update `openspec/specs/microsoft-auth/spec.md`: change keyring service name from "filesync" to "cloudmount" and default client ID reference from "FileSync" to "CloudMount"
- [x] 10.2 Update `openspec/specs/config-persistence/spec.md`: change all config paths from `filesync` to `cloudmount`, service names from `filesync.service` to `cloudmount.service`, LaunchAgent from `com.filesync.agent` to `com.cloudmount.agent`
- [x] 10.3 Update `openspec/specs/packaged-defaults/spec.md`: change default app name references from "FileSync" to "CloudMount"
- [x] 10.4 Update `openspec/specs/virtual-filesystem/spec.md`: change default Windows mount path from `FileSync` to `CloudMount`
- [x] 10.5 Update `openspec/specs/tray-app/spec.md`: change default branding references from "FileSync" to "CloudMount"

## 11. Update AGENTS.md knowledge base files

- [x] 11.1 Update root `AGENTS.md`: replace all `filesync-*` crate references with `cloudmount-*`, update paths, commands, and descriptions
- [x] 11.2 Update `crates/cloudmount-auth/AGENTS.md`: replace `filesync` references with `cloudmount`
- [x] 11.3 Update `crates/cloudmount-cache/AGENTS.md`: replace `filesync` references with `cloudmount`
- [x] 11.4 Update `crates/cloudmount-vfs/AGENTS.md`: replace `filesync` references with `cloudmount`

## 12. Verify and clean up

- [x] 12.1 Run `cargo clean` to remove stale incremental build artifacts
- [x] 12.2 Run `cargo build --all-targets` and fix any compile errors
- [x] 12.3 Run `cargo test --all-targets` and fix any test failures
- [x] 12.4 Run `cargo clippy --all-targets --all-features` and fix any warnings
- [x] 12.5 Run `cargo fmt --all -- --check` and fix any formatting issues
- [x] 12.6 Run a final grep sweep for any remaining `filesync` or `FileSync` occurrences outside of `openspec/changes/archive/` and fix them
