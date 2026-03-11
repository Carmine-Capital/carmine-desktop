## Context

CloudMount mounts OneDrive/SharePoint drives as local filesystems. When a user opens an Office document from the mount, it opens as a local file — no co-authoring, no auto-save to server. Conflicts are detected after the fact via eTag comparison, producing `.conflict` copies.

Microsoft Office supports co-authoring natively when files are opened via SharePoint URLs. The Graph API returns a `webUrl` field on every `DriveItem`, but CloudMount currently excludes it from `$select` queries and doesn't have the field on `DriveItem`. The data is available at zero cost — it just isn't captured.

CloudMount already has URL-opening infrastructure (`open_with_clean_env` / `open::that` in `main.rs:29-47`) and a Tauri command system. No deep-link plugin is currently installed (current plugins: dialog, notification, updater, process, opener).

## Goals / Non-Goals

**Goals:**
- Let users open any mounted file in SharePoint/Office Online with one action
- On Windows/macOS, open Office documents directly in the desktop Office app with co-authoring via Office URI schemes (`ms-word:ofe|u|...`)
- Provide a shell-integrated trigger on Windows (Explorer right-click context menu)
- Provide a fallback trigger on Linux (Nautilus script)
- Capture `webUrl` on every `DriveItem` with zero additional API calls

**Non-Goals:**
- Implementing MS-FSSHTTP/COPS or any co-authoring protocol
- Automatic interception of double-click on Office files (future Option A: `IStorageProviderUriSource`)
- Making double-click behavior change — this is always an explicit user action
- Thumbnail providers, custom states, or other CfApi shell extensions
- Offline support for the "Open in SharePoint" action

## Decisions

### 1. `webUrl` via `$select` extension, not on-demand fetching

**Decision:** Add `webUrl` to the existing `$select` parameter in `list_children()` and `list_root_children()`. Store it as `Option<String>` on `DriveItem`.

**Rationale:** The Graph API already returns this data — it's excluded only because `$select` doesn't request it. Adding one field to `$select` has negligible bandwidth impact (a URL string per item). The alternative — fetching `webUrl` on demand via `get_item()` when the user clicks "Open Online" — adds latency and an API call at the moment the user expects instant feedback.

**Note:** `delta_query()` and `get_item()` don't use `$select`, so they already return `webUrl` from the server. Once the struct field exists, these calls will capture it automatically via serde deserialization.

### 2. Office URI schemes for desktop co-authoring on Windows/macOS

**Decision:** Map Office file extensions to their URI schemes:

| Extensions | URI Scheme | Opens |
|---|---|---|
| `.doc`, `.docx`, `.docm` | `ms-word:ofe\|u\|<webUrl>` | Word (edit mode) |
| `.xls`, `.xlsx`, `.xlsm` | `ms-excel:ofe\|u\|<webUrl>` | Excel (edit mode) |
| `.ppt`, `.pptx`, `.pptm` | `ms-powerpoint:ofe\|u\|<webUrl>` | PowerPoint (edit mode) |
| Everything else | `<webUrl>` (plain HTTPS) | Default browser |

**Rationale:** Office URI schemes open the desktop app with a direct SharePoint connection — full co-authoring, real-time cursors, auto-save. This is the same mechanism Office Online's "Open in Desktop App" button uses. `open::that()` (already in the codebase) handles these URIs natively on Windows/macOS.

**Alternative considered:** Always open in browser. Simpler, but loses the desktop Office experience that Windows/macOS users expect. The URI scheme approach gives the best-available experience per platform with minimal code.

On Linux, always fall back to the browser URL since desktop Office isn't available.

### 3. Tauri deep-link protocol for shell integration

**Decision:** Register a `cloudmount://` URL protocol handler via the Tauri `deep-link` plugin. The Windows context menu entry invokes `cloudmount://open-online?path=<encoded-path>`, which the running Tauri app handles.

**Rationale:** This avoids building a separate companion binary or COM DLL for the context menu. The Tauri app is always running (tray), so it can handle the request immediately. The deep-link plugin is a standard Tauri v2 plugin. The same protocol works across platforms.

**Alternative considered:** A standalone `cloudmount-open.exe` CLI tool. Would work without the Tauri app running, but requires a separate binary, its own way to resolve paths to `webUrl` (either IPC or direct cache access), and additional build/packaging complexity. Deferred as unnecessary for v1.

### 4. Windows context menu via registry, not COM shell extension

**Decision:** Register a context menu entry for files inside CfApi sync roots using Windows registry keys. The entry runs `start cloudmount://open-online?path=%1`.

**Rationale:** A full `IContextMenu` COM shell extension would allow dynamic visibility (only show for CloudMount files), but adds significant complexity — a separate DLL, COM registration, in-process loading into Explorer. A registry-based entry is a few registry writes during mount setup and can be scoped to the CloudMount sync root directory using `Directory\Background\shell` or applied to specific file types. The trade-off is that the entry may appear outside CloudMount directories, but the deep-link handler validates the path and shows an error if the file isn't in a mount.

**Alternative considered:** CfApi custom actions. The `cloud-filter` crate (v0.0.6) does not expose custom action APIs, so this would require raw `windows` crate COM calls. Deferred until the crate supports it or Option A (`IStorageProviderUriSource`) is pursued.

### 5. Path resolution reuses existing `CoreOps::resolve_path`

**Decision:** The Tauri command receives an absolute local path, strips the mount prefix, splits into path components, and calls `CoreOps::resolve_path()` which returns `(inode, DriveItem)`. The `DriveItem.web_url` field provides the URL. No new resolution mechanism needed.

**Rationale:** `resolve_path` (`core_ops.rs:559`) already does exactly this — walks the inode table component by component, falling back through memory → SQLite → Graph API. It's used by every CfApi callback today. The only new part is exposing it through a Tauri command.

## Risks / Trade-offs

**[Risk] Office URI scheme not installed** → On systems without Office installed, `ms-word:ofe|u|...` will fail to open. Mitigation: detect failure from `open::that()` and fall back to the plain `webUrl` (opens in browser/Office Online). The browser fallback always works.

**[Risk] Context menu appears outside CloudMount directories** → Registry-based context menus can't be perfectly scoped to a single directory tree. Mitigation: the deep-link handler validates that the path is inside a known mount point and shows a notification error if not. The menu entry text ("Open in SharePoint") provides a contextual hint. Can be refined later with a proper shell extension.

**[Risk] `webUrl` not populated for cached items** → Items cached before the field was added won't have `webUrl` until re-fetched (delta sync or TTL expiry). Mitigation: the memory cache TTL is 60 seconds, and delta sync runs periodically. Worst case, the Tauri command falls back to `get_item()` which always returns `webUrl`. Freshly-opened directories will have it immediately.

**[Risk] Tauri app not running when context menu is clicked** → The deep-link protocol handler requires the app to be running. Mitigation: CloudMount is designed to run as a persistent tray app. If not running, the OS will attempt to launch it (protocol handler registration points to the app binary). Acceptable for v1.

**[Risk] Deep-link URL encoding edge cases** → File paths with unicode, spaces, or special characters must be properly encoded in the `cloudmount://` URL. Mitigation: use percent-encoding on the path parameter and decode on receipt. Standard URL encoding, well-supported in Rust.
