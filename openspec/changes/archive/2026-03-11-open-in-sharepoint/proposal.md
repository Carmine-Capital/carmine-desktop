## Why

carminedesktop users who open Office documents (.docx, .xlsx, .pptx) from the local mount get a degraded experience: the file opens as a local copy with no co-authoring, no real-time cursors, and conflicts are resolved via `.conflict` copies. Meanwhile, the same file opened through SharePoint gives full co-authoring with automatic merge. carminedesktop already knows the SharePoint URL of every file — it just doesn't expose it. Giving users a one-click "Open in SharePoint" action bridges this gap with zero protocol implementation by letting Office and the browser handle co-authoring natively.

## What Changes

- Add `webUrl` field to `DriveItem` and include it in Graph API `$select` queries, so every cached item carries its SharePoint URL.
- Add a Tauri command (`open_online`) that resolves a local mount path to its `webUrl` and opens it — using Office URI schemes (`ms-word:ofe|u|...`) on Windows/macOS for desktop Office co-authoring, or the browser on Linux.
- Register a `carminedesktop://` deep-link protocol so external tools (shell context menus, scripts) can trigger "Open in SharePoint" for a given path.
- On Windows, add a shell context menu entry ("Open in SharePoint") for files inside CfApi sync roots, wired to the deep link.
- On Linux, provide a Nautilus script as a lightweight equivalent.

## Capabilities

### New Capabilities
- `open-in-sharepoint`: Resolve local mount paths to SharePoint URLs and open them in the browser or desktop Office with co-authoring support. Covers the Tauri command, Office URI scheme mapping, deep-link protocol, and platform-specific shell integration (Windows context menu, Linux Nautilus script).

### Modified Capabilities
- `graph-client`: List-children and list-root-children responses must include the `webUrl` field for each DriveItem.

## Impact

- **carminedesktop-core**: `DriveItem` struct gains one `Option<String>` field.
- **carminedesktop-graph**: `$select` parameter in `list_children` / `list_root_children` gains `webUrl`. No new API calls.
- **carminedesktop-app**: New Tauri command, deep-link handler registration, platform-specific shell integration setup.
- **carminedesktop-vfs**: No changes — `CoreOps::resolve_path` already returns `DriveItem`, which will now carry `webUrl`.
- **Dependencies**: Tauri `deep-link` plugin (if not already present). No new external crates expected for the Windows context menu (registry-based approach).
- **Breaking changes**: None. The `webUrl` field is additive and optional.
