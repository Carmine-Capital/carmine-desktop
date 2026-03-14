## Why

Mounted drives (OneDrive, SharePoint libraries) currently appear only as regular directories under `~/Cloud/` with no special presence in Windows Explorer's navigation pane. Users must manually navigate to the mount path or pin it to Quick Access. Cloud storage competitors (OneDrive native, Google Drive, Dropbox) all register as dedicated entries in the navigation pane, making their content instantly discoverable. Carmine Desktop should match this UX expectation.

## What Changes

- Register a persistent "Carmine Desktop" root node in Windows Explorer's left navigation pane as a cloud storage provider (delegate folder CLSID pointing to the Cloud root directory).
- Individual mount directories appear as children of the root node, dynamically matching mount start/stop lifecycle.
- Clicking the root node when the app is not running launches Carmine Desktop via `shell\open\command`.
- The root node displays the Carmine Desktop icon; children use standard folder icons.
- Auto-cleanup of child entries on app close; full registry cleanup on uninstall or when disabled via settings toggle.
- `SHChangeNotify` called after every registry mutation to refresh Explorer.
- Stale entry detection and cleanup on startup (handles prior crashes).

## Capabilities

### New Capabilities
- `windows-explorer-nav-pane`: Registration, lifecycle management, and cleanup of a delegate folder CLSID in Windows Explorer's navigation pane, including root node persistence, dynamic child visibility, app launch on click, and settings toggle.

### Modified Capabilities
- `app-lifecycle`: Setup and teardown hooks for nav pane registration/unregistration during startup and graceful shutdown.
- `config-persistence`: New `explorer_nav_pane: bool` setting (default `true` on Windows).

## Impact

- **Code**: `shell_integration.rs` (new registration/unregistration functions), `main.rs` (lifecycle hooks), `config.rs` (new setting), `commands.rs` (Tauri command for toggle), `dist/` (settings UI toggle).
- **Dependencies**: Uses existing `winreg` crate and `windows` crate APIs (`SHChangeNotify`). No new dependencies expected.
- **Platform**: Windows-only (`#[cfg(target_os = "windows")]`). No impact on Linux/macOS.
- **Registry**: Writes to `HKCU\Software\Classes\CLSID\{GUID}`, `HKCU\...\Explorer\Desktop\NameSpace\{GUID}`, `HKCU\...\HideDesktopIcons\NewStartPanel\{GUID}`.
