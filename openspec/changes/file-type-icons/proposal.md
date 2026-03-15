## Why

When Carmine Desktop registers as the default handler for Office file types, Windows replaces the native Word/Excel/PowerPoint icons with the generic Carmine Desktop application icon for all files of those types system-wide. This creates a confusing experience where users can no longer visually distinguish file types at a glance in Explorer.

## What Changes

- Embed per-file-type `.ico` resources (doc, xls, ppt, pdf) into the Windows executable using a resource script compiled via `embed-resource`
- Set `DefaultIcon` registry subkeys during file association registration so each ProgID displays its correct file-type icon instead of the generic app icon
- Clean up `DefaultIcon` entries during unregistration (already handled by existing `delete_subkey_all`)
- Convert existing SVG icons in `icons/files/` to multi-resolution `.ico` files (16, 32, 48, 256px)

## Capabilities

### New Capabilities
- `file-type-icons`: Embedding per-file-type icon resources in the Windows executable and setting them as DefaultIcon during file association registration

### Modified Capabilities

(none — no existing spec-level requirements change)

## Impact

- **Build system**: `build.rs` gains an `embed_resource::compile()` call for a new `.rc` resource script (Windows only)
- **Dependencies**: `embed-resource` added as explicit build-dependency (already in `Cargo.lock` transitively via `tauri-build` → `tauri-winres`)
- **Shell integration**: `register_file_associations()` in `shell_integration.rs` writes `DefaultIcon` subkey per ProgID
- **Assets**: New `.ico` files and `.rc` file in `crates/carminedesktop-app/icons/files/`
- **Platforms**: Windows only. macOS and Linux unaffected.
