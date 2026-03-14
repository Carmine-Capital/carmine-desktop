## Context

Carmine Desktop mounts OneDrive and SharePoint libraries as local filesystem directories via WinFsp on Windows. These mounts live under a configurable root directory (default `~/Cloud/`), appearing as regular folders. There is no presence in Windows Explorer's navigation pane — users must manually navigate to the mount path. Competing cloud storage apps (OneDrive native, Google Drive, Dropbox) all register as dedicated entries in the navigation pane.

Existing shell integration (`shell_integration.rs`) handles Office file type associations via per-user registry keys (HKCU). The same module and registry patterns will be extended for navigation pane integration.

WinFsp mount directories are created at mount start and removed at unmount. The root `~/Cloud/` directory persists across app sessions, making it a natural target for a persistent delegate folder.

## Goals / Non-Goals

**Goals:**
- Register a "Carmine Desktop" root node in Windows Explorer's navigation pane as a cloud storage provider entry
- Show individual mount directories as children of the root node, appearing/disappearing with mount lifecycle
- Allow launching Carmine Desktop by clicking the root node when the app is not running
- Provide a settings toggle to enable/disable the feature
- Clean up all registry entries on disable or uninstall
- Handle stale entries from previous crashes on startup

**Non-Goals:**
- Custom icons per child mount (standard folder icons are sufficient)
- Shell namespace extension (COM DLL) — too complex for the benefit
- Cloud Files API (CfApi) integration — incompatible with WinFsp architecture
- Sync status badges or overlays on files
- Context menu entries on the navigation pane node
- Linux/macOS equivalents (future work)

## Decisions

### D1: Delegate Folder via CLSID registration

**Decision:** Use a Windows "delegate folder" — a CLSID registered in HKCU that points to a real filesystem directory via `TargetFolderPath`.

**Alternatives considered:**
- *Shell Namespace Extension (COM DLL)*: Maximum control but requires a separate DLL, COM registration, and is fragile (bugs crash Explorer). Overkill for pointing at an existing directory.
- *Cloud Files API sync root*: Gives native navigation pane integration with badges, but is fundamentally incompatible with WinFsp (different filesystem model).
- *Pin to Quick Access*: Simple but mixes with user's own pins, no branding, no grouping.

**Rationale:** The delegate folder approach requires zero compiled code beyond registry writes. Since WinFsp already creates real directories under `~/Cloud/`, Explorer naturally shows them as children when expanding the delegate folder. This is the simplest correct solution.

### D2: Fixed CLSID GUID

**Decision:** Use a hardcoded GUID `{E4B3F4A1-7C2D-4A8E-B5D6-9F1E2A3C4B5D}` for the root CLSID. Generate it once, commit it, never change it.

**Rationale:** A stable GUID ensures the navigation pane entry survives app updates. Generating at runtime would create duplicates. The GUID is arbitrary — it just needs to be unique and stable.

### D3: Registry structure

**Decision:** Three registry locations per the Windows shell delegate folder pattern:

1. **CLSID definition** — `HKCU\Software\Classes\CLSID\{GUID}`
   - `(Default)` = `"Carmine Desktop"`
   - `DefaultIcon\(Default)` = `"<exe_path>,0"` (app icon)
   - `InProcServer32\(Default)` = `"%SystemRoot%\\system32\\shell32.dll"` (delegate to shell)
   - `Instance\CLSID` = `{0E5AAE11-A475-4c5b-AB00-C66DE400274E}` (delegate folder class)
   - `Instance\InitPropertyBag\TargetFolderPath` = `"C:\Users\<user>\Cloud"`
   - `Instance\InitPropertyBag\Attributes` = `0x11` (SFGAO_FOLDER | SFGAO_HASSUBFOLDER)
   - `ShellFolder\FolderValueFlags` = `0x28` (hide in bread crumb)
   - `ShellFolder\Attributes` = `0xF080004D` (standard navigation pane attributes)
   - `shell\open\command\(Default)` = `"<exe_path>"` (launch app on click)

2. **Desktop namespace pin** — `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{GUID}`
   - `(Default)` = `"Carmine Desktop"`

3. **Hide from desktop** — `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel\{GUID}`
   - `(Default)` = DWORD `1`

**Rationale:** This is the documented pattern for cloud storage providers. The `InProcServer32` pointing to `shell32.dll` with the delegate folder instance CLSID tells Explorer to resolve `TargetFolderPath` as the folder contents. The `ShellFolder\Attributes` value controls navigation pane visibility. `HideDesktopIcons` prevents the entry from also appearing on the desktop.

### D4: Dynamic children via filesystem reality

**Decision:** Do NOT register child CLSIDs in the registry. Children are real directories created/removed by WinFsp — Explorer shows them automatically when expanding the delegate folder.

**Rationale:** This eliminates all child registry management complexity. When a mount starts, WinFsp creates `~/Cloud/OneDrive/`; Explorer sees it. When the mount stops, the directory is removed; Explorer stops showing it. The hybrid lifecycle (persistent root, dynamic children) is achieved without any additional code for child management.

### D5: App launch via shell\open\command

**Decision:** Register `shell\open\command` on the CLSID pointing to the Carmine Desktop executable. When the user clicks the root node and the app is not running, Explorer launches the app. When already running, Explorer simply navigates into the delegate folder.

**Rationale:** Simpler than a custom protocol URI. No additional protocol registration needed. The executable path is already known (same as auto-start registration). The behavior degrades gracefully — if the exe is missing, Windows shows a standard "file not found" error.

### D6: SHChangeNotify after every mutation

**Decision:** Call `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None)` after every registry write/delete batch to force Explorer to re-read the navigation pane.

**Rationale:** Without this, Explorer caches the navigation pane state and may not reflect changes until next login. The existing file association code already uses this pattern.

### D7: Config setting with reconciliation on startup

**Decision:** Add `explorer_nav_pane: Option<bool>` to `UserGeneralSettings`, resolving to `true` by default on Windows. Follow the same pattern as `register_file_associations` and `auto_start` — apply immediately on save, reconcile on startup.

**Rationale:** Matches existing config patterns exactly. Reconciliation on startup handles stale state (e.g., user manually deleted registry keys, or app crashed before cleanup).

### D8: Code organization

**Decision:** Add all navigation pane functions to the existing `shell_integration.rs` module. New functions:
- `register_nav_pane(cloud_root: &Path) -> Result<()>` — creates all 3 registry entries
- `unregister_nav_pane() -> Result<()>` — removes all 3 registry entries
- `is_nav_pane_registered() -> bool` — checks if CLSID exists
- `update_nav_pane_target(cloud_root: &Path) -> Result<()>` — updates TargetFolderPath if root_dir changed

**Rationale:** Shell integration code belongs together. The file is already Windows-gated. Adding ~100 lines to an existing module is cleaner than a new file.

## Risks / Trade-offs

- **[Windows version compatibility]** → The delegate folder pattern works on Windows 10 1809+ and Windows 11. Older versions may not show the entry. Mitigation: document minimum Windows version; the entry is non-functional but harmless on older versions.
- **[Explorer refresh latency]** → SHChangeNotify may take 1-2 seconds to propagate. Mitigation: acceptable UX; same behavior as OneDrive/Dropbox.
- **[Stale root node after crash]** → If the app crashes, the root node persists (by design) but children are gone (WinFsp unmounted). Mitigation: this is the desired behavior — clicking the root node relaunches the app.
- **[TargetFolderPath stale after root_dir change]** → If the user changes `root_dir` in settings, the delegate folder points to the old path. Mitigation: `update_nav_pane_target()` called in `save_settings` when root_dir changes.
- **[Registry permission errors]** → HKCU writes should always succeed for the current user. Mitigation: non-fatal error handling with warning log, same as file associations.
