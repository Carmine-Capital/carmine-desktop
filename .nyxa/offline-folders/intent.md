# Intent: Make a folder available offline via Explorer context menu

## Context

CarmineDesktop mounts OneDrive/SharePoint document libraries as local filesystems. Files are currently loaded on demand. The user wants to mark an entire folder for persistent offline sync, directly from the Windows Explorer context menu.

## Feature

### Windows Explorer Context Menu

- Add a **"Make available offline"** entry in the right-click context menu for folders on the CarmineDesktop VFS mount.
- Add a **"Free up space"** entry (or "Remove offline availability") for already-pinned folders, allowing manual unpin before TTL expiry.
- Implementation via **static registry verbs** (simple approach, no COM `IContextMenu` DLL).
- The context menu always appears on VFS mount folders — size filtering is done at execution time, not at display time.

### Execution Behavior

1. User right-clicks a folder → selects "Make available offline".
2. The registry verb launches a command (or communicates with the Tauri app via IPC/named pipe).
3. The app checks the folder size via the `DriveItem.size` field (recursive size already provided by Microsoft Graph and stored in cache).
4. **If size exceeds 5 GB**: the operation is rejected and a system notification informs the user (e.g., "This folder is too large (X GB). Maximum is 5 GB.").
5. **If size is acceptable**: the folder is marked as "pinned offline" with a TTL.
6. The folder contents (recursive) are downloaded and persistently synced.
7. A system notification confirms download completion (or reports an error).

### Persistent Sync with TTL

- Offline pinning is **temporary** with a configurable duration (TTL).
- **Default duration**: 1 day.
- **Configurable** in app settings.
- **Maximum**: 7 days.
- While TTL is active, the folder stays synced (remote changes reflected locally, local changes uploaded).
- On TTL expiry, the folder reverts to on-demand mode (files may be evicted by normal LRU cache policy).

### Manual Unpin

- The "Free up space" entry removes offline pinning before TTL expiry.
- Files revert to on-demand mode immediately.

### User Feedback

- System notification on initial download completion (success or error).
- System notification on rejection (folder too large).

## Constraints

- **Size limit**: 5 GB max per pinned folder. Checked at execution time via `DriveItem.size`.
- **Max duration**: 7 days TTL.
- **No dynamic menu filtering**: context menu always appears; validation is done app-side.

## Platforms

- **Windows**: required (v1). Implementation via static registry verbs.
- **Linux / macOS**: desired if low effort and implementation is shareable. Not blocking.

## Out of Scope

- COM `IContextMenu` DLL for dynamic menu filtering.
- Advanced pinned-folder management UI in Tauri (may come later).
- Per-folder duration choice at pin time (uses default from settings).
