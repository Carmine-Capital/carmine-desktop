## Why

The `collaborative-file-opening` change introduced a CollabGate dialog that asks users "Open Online or Open Locally?" every time they double-click an Office file. This creates friction in the common case — most users want online editing — and introduces complexity (per-extension preferences, config UI, "Remember my choice" checkbox, forced dialog on local changes) that isn't justified pre-launch. If the system always opens collaborative files online by default, local edits never accumulate, so the scenarios the dialog was protecting against (stale local changes) don't arise. The context menu entries ("Open Online" / "Open Locally") are also redundant when the behavior is automatic. A future feature will handle the "download locally for offline work" use case explicitly as a separate mechanism.

## What Changes

- **Remove CollabGate dialog**: The native "Open Online / Open Locally" popup is removed entirely. CollabGate still intercepts opens from interactive shells, but always resolves to `OpenOnline` without asking.
- **Remove context menu entries on all platforms**: Linux (Nautilus scripts, KDE service menus, helpers), Windows (Explorer registry entries), and macOS (placeholder) context menu entries for "Open Online" and "Open Locally" are deleted. No migration cleanup needed (pre-production).
- **Remove collaborative editing settings UI**: The entire "Collaborative Editing" section in the Settings page (enabled toggle, default action dropdown, per-extension preferences, timeout) is removed.
- **Simplify config structure**: `CollabDefaultAction` enum and per-extension overrides are removed. `CollaborativeOpenConfig` retains only `timeout_seconds` and `shell_processes` as power-user TOML-only settings.
- **Clean up dead code**: `CollabOpenResponse::Cancel` variant, `VfsError::OperationCancelled`, `has_local_changes` detection, `resolve_collab_preference()`, `show_collab_dialog()`, and all associated mappings are removed.
- **Remove tray menu integration toggle**: The "Install/Remove file manager integration" item in the Linux system tray menu is removed along with its notifications.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `collaborative-open-gate`: Remove dialog, preference resolution, config UI, `Ask`/`Cancel` variants, `has_local_changes` guard, and per-extension overrides. CollabGate always resolves to online open for collaborative files from interactive shells. Config shrinks to `timeout_seconds` + `shell_processes` only.
- `open-in-sharepoint`: Remove all context menu entries (Nautilus, KDE, Windows Explorer). Core `open_online` command and deep-link handler remain as internal mechanism for CollabGate's online open path.
- `windows-context-menu-lifecycle`: Remove entirely — no context menu entries to lifecycle-manage.
- `kde-open-in-sharepoint`: Remove entirely — no KDE context menu entry.
- `virtual-filesystem`: Remove `VfsError::OperationCancelled` variant and its platform mappings. Remove `CollabOpenResponse::Cancel` handling from CollabGate match. Remove `has_local_changes` computation from `CollabOpenRequest`.
- `tray-app`: Remove "Install/Remove file manager integrations" menu item and its handler from the Linux tray menu.

## Impact

- **carminedesktop-core** (config.rs, types.rs): Remove `CollabDefaultAction` enum, `CollabOpenResponse::Cancel`, `has_local_changes` from `CollabOpenRequest`, simplify `CollaborativeOpenConfig`
- **carminedesktop-vfs** (core_ops.rs, fuse_fs.rs, winfsp_fs.rs): Remove `OperationCancelled` error variant and mappings, remove `Cancel` arm in CollabGate, stop computing `has_local_changes`
- **carminedesktop-app** (main.rs, commands.rs, tray.rs, notify.rs): Remove `show_collab_dialog()`, `resolve_collab_preference()`, collab config commands, integration toggle, integration notifications. Simplify `spawn_collab_handler()` to always-online path.
- **carminedesktop-app** (linux_integrations.rs, windows_integrations.rs, macos_integrations.rs): Delete modules entirely
- **carminedesktop-app/scripts/**: Delete all 7 context menu scripts
- **carminedesktop-app/dist/** (settings.html, settings.js): Remove "Collaborative Editing" settings section
- **carminedesktop-vfs/tests/**: Update CollabGate tests to remove Cancel/dialog/preference scenarios
