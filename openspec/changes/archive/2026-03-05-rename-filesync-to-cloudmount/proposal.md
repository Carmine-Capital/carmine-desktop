## Why

The project repository is named "carminedesktop" but the application, all 6 crates, internal identifiers, config paths, service names, and documentation still use the old "FileSync" / "filesync" naming. This creates confusion — the repo name says one thing, the code says another. Aligning everything under the "carminedesktop" / "carminedesktop" identity establishes a consistent brand before the first public release.

## What Changes

- **BREAKING**: Rename all 6 Rust crate packages from `filesync-*` to `carminedesktop-*` (e.g., `filesync-core` → `carminedesktop-core`), including directory names under `crates/`
- **BREAKING**: Rename the Rust module paths from `filesync_*` to `carminedesktop_*` across all `use` statements, return types, and error references
- **BREAKING**: Change OS config paths from `filesync` to `carminedesktop` (e.g., `~/.config/filesync/` → `~/.config/carminedesktop/`)
- **BREAKING**: Change keyring service name from `"filesync"` to `"carminedesktop"` and encrypted token directory from `filesync/` to `carminedesktop/`
- **BREAKING**: Change default app display name from `"FileSync"` to `"carminedesktop"`
- Change Tauri product name and identifier (`com.filesync.app` → `com.carminedesktop.app`)
- Change platform service names: `filesync.service` → `carminedesktop.service`, `com.filesync.agent` → `com.carminedesktop.agent`
- Change SQLite database filename from `filesync.db` to `carminedesktop.db`
- Change VFS identifiers: FUSE `FSName`, CfApi `PROVIDER_NAME`, tray icon ID
- Change Windows registry key from `"FileSync"` to `"carminedesktop"`
- Rename internal Rust structs: `FileSyncFs` → `carminedesktopFs`, `FileSyncCfFilter` → `carminedesktopCfFilter`
- Update all documentation: README, builder guide, Azure AD setup guide
- Update CI/CD workflows to reference new crate names
- Update HTML templates (wizard.html, settings.html) with new branding
- Update all OpenSpec main specifications to reflect new naming

## Capabilities

### New Capabilities

_None — this change introduces no new functionality._

### Modified Capabilities

- `microsoft-auth`: Keyring service name changes from `"filesync"` to `"carminedesktop"`; encrypted token path changes from `filesync/` to `carminedesktop/`
- `config-persistence`: All config directory paths change from `filesync` to `carminedesktop`; systemd service name, macOS LaunchAgent identifier, and Windows registry key rename
- `packaged-defaults`: Default app name constant changes from `"FileSync"` to `"carminedesktop"`; example branding updated
- `virtual-filesystem`: FUSE FSName, CfApi provider name, and default Windows mount path change from `FileSync` to `carminedesktop`
- `tray-app`: Tray icon ID changes from `"filesync-tray"` to `"carminedesktop-tray"`; HTML template titles updated

## Impact

- **Crate structure**: All 6 crate directories renamed; all 7 `Cargo.toml` files updated (workspace root + 6 crates)
- **Rust source**: ~345 occurrences across 27 `.rs` files — `use` statements, module paths, error types, constants, string literals
- **Config paths**: Users of pre-release builds will need to manually migrate `~/.config/filesync/` → `~/.config/carminedesktop/` (no automated migration for pre-v1)
- **Token storage**: Existing keyring entries under `"filesync"` service will be orphaned; users must re-authenticate after upgrade
- **CI/CD**: Workflow files reference `filesync-vfs` and `filesync-app` by name
- **Documentation**: README, 2 guide files, build defaults, 9 spec files, AGENTS.md files
- **HTML assets**: 2 webview template files with window titles
- **No external API changes**: Microsoft Graph endpoints, OAuth flows, and FUSE/CfApi protocols are unaffected
