## Why

The "Add Mount" button in Settings and the SharePoint source selection in the setup wizard are both stubs — clicking either does nothing useful. A user who wants to add a SharePoint library has no way to do so from the UI, making SharePoint support effectively unreachable despite all backend infrastructure (`search_sites`, `list_drives`, `add_mount`) being fully implemented. This is a blocking gap that must be closed before the app is usable for its primary SharePoint use-case.

## What Changes

- **wizard.html `selectSource()`**: Replace the two-line stub with real logic. OneDrive path calls `add_mount` directly (drive ID from `complete_sign_in` context). SharePoint path navigates to a new `step-sharepoint` wizard step.
- **wizard.html `step-sharepoint` step**: New wizard step with a search input that calls `search_sites`, displays results, lets the user pick a site, calls `list_drives`, displays libraries, lets the user pick a library, then calls `add_mount`. Inline error and empty-state handling throughout.
- **wizard.html single-library auto-select**: When a site has exactly one document library, auto-select it and skip the library-selection sub-step (per existing sharepoint-browser spec).
- **wizard.html `step-done` refresh**: After `add_mount` succeeds, refresh mount list display in `step-done` so the new mount is visible.
- **settings.html `addMount()`**: Replace the empty stub with a call to `window.__TAURI__.window.WebviewWindow` (or `open_or_focus_window` via a new Tauri command) to open the wizard window, allowing the user to add mounts post-setup.

## Capabilities

### New Capabilities

- `sharepoint-wizard`: The end-to-end UI flow for discovering and mounting a SharePoint document library within the wizard — search, site selection, library selection, mount confirmation.

### Modified Capabilities

- `sharepoint-browser`: Delta spec to add the missing UI-level requirements for the wizard step flow (search interaction, site-list rendering, library-list rendering, single-library auto-selection, add-mount invocation). The backend requirements already exist; what is missing are the wizard-step interaction requirements.
- `tray-app`: Delta spec to fix the `addMount()` scenario in Settings — currently the spec says users can add mounts from the Mounts tab but the implementation is a stub. The requirement for `addMount()` to open the wizard must be made explicit.

## Impact

- **`crates/carminedesktop-app/dist/wizard.html`**: New `step-sharepoint` DOM step, `selectSource()` rewrite, new JS functions (`searchSites`, `selectSite`, `selectLibrary`, `confirmMount`), CSS additions for search input, results list, back button, loading/error states.
- **`crates/carminedesktop-app/dist/settings.html`**: `addMount()` rewrite to open wizard window via Tauri API or a new `open_wizard` command.
- **`crates/carminedesktop-app/src/commands.rs`**: Possibly a new `open_wizard` command if the JS WebviewWindow API is not sufficient to focus/open the wizard from the settings window context. No changes to existing commands.
- **No backend changes**: `search_sites`, `list_drives`, and `add_mount` commands are already implemented and registered.

## Dependencies

- **fix-ui-feedback** MUST be applied before this change. The `addMount()` rewrite in `settings.html` (task 2.1) calls `showStatus()` which is introduced by `fix-ui-feedback`. Applying this change without `fix-ui-feedback` will produce a `ReferenceError` at runtime when `addMount()` fails.
- **fix-settings-xss** SHOULD be applied before this change. If `fix-settings-xss` task 3.3 was triggered (the Tauri webview blocked inline scripts and they were extracted to `wizard.js`), then all new JavaScript added by this change to `wizard.html` must instead be added to `wizard.js`. If task 3.3 was not required, inline `<script>` blocks remain unchanged and no adjustment is needed.
