## Why

Users who mount OneDrive/SharePoint document libraries via CloudMount currently open files locally by default. This means Office files (Word, Excel, PowerPoint) are downloaded and edited in isolation, missing out on real-time co-authoring that SharePoint provides natively. Users must manually navigate to SharePoint in a browser to collaborate. CloudMount should make collaborative editing the natural default for files that support it, while preserving local access for everything else.

## What Changes

- **VFS-level interception (CollabGate)**: When a user double-clicks a collaborative-capable file on a CloudMount mount, the VFS intercepts the open, identifies the caller process (shell vs indexer), and presents a choice dialog before serving the file.
- **Native dialog for open mode selection**: Tauri app receives open requests from the VFS and shows a native dialog — "Open Locally" / "Open Online" — with a "Remember my choice" option per extension. Warns about unsynchronized local modifications when present.
- **Process-aware filtering**: Only interactive shell processes (Explorer, Nautilus, Dolphin, Finder) trigger the collaborative open dialog. Indexers, antivirus, and thumbnailers bypass it silently.
- **Per-extension user preferences**: Users can set a default open mode per file extension (or globally), stored in CloudMount config. Once set, the dialog is skipped.
- **Enhanced context menu**: Right-click menu gains both "Open Online" and "Open Locally" entries (currently only "Open in SharePoint" exists), giving users explicit control.
- **Cross-platform collaborative open**: Office URI schemes (`ms-word:ofe|u|<webUrl>`) on Windows/macOS for desktop co-authoring; browser fallback on Linux and for non-Office collaborative files.

## Capabilities

### New Capabilities
- `collaborative-open-gate`: VFS-level interception mechanism (CollabGate) — process filtering, async channel to Tauri, timeout handling, file type classification, and user preference resolution for collaborative vs local open decisions.

### Modified Capabilities
- `open-in-sharepoint`: Extend with dual context menu entries ("Open Online" / "Open Locally") and integrate as the backend for CollabGate's online open path.
- `virtual-filesystem`: Add CollabGate hook in the `open()` path — before serving content, check if the file is collaborative-capable and the caller is an interactive shell.
- `windows-context-menu-lifecycle`: Register two entries instead of one — `CloudMount.OpenOnline` and `CloudMount.OpenLocally` — with the same reference-counted lifecycle.

## Impact

- **cloudmount-vfs** (core_ops.rs, winfsp_fs.rs, fuse_fs.rs): CollabGate logic, process filtering, channel communication with Tauri app
- **cloudmount-app** (main.rs, commands.rs): Dialog handling, preference management, CollabGate event listener
- **cloudmount-core** (types, config, open_online.rs): New types (CollabOpenRequest/Response), file type classification (`is_collaborative()`), config additions for collaborative open preferences
- **cloudmount-app/src/linux_integrations.rs**: Enhanced Nautilus/KDE scripts with dual menu entries
- **User config**: New `[collaborative_open]` section with `enabled`, `default_action`, per-extension overrides, timeout
