## 1. Rust: open_wizard command

- [x] 1.1 Ensure `open_or_focus_window` in `crates/carminedesktop-app/src/tray.rs` is `pub(crate)` (or `pub`) so it can be called from `commands.rs`
- [x] 1.2 Add `#[tauri::command] pub async fn open_wizard(app: tauri::AppHandle) -> Result<(), String>` to `crates/carminedesktop-app/src/commands.rs` that calls `crate::tray::open_or_focus_window(&app, "wizard", "Setup", "wizard.html").map_err(|e| e.to_string())`
- [x] 1.3 Register `open_wizard` in the `invoke_handler!` macro in `crates/carminedesktop-app/src/main.rs`
- [x] 1.4 Run `cargo build -p carminedesktop-app --features desktop` and confirm it compiles cleanly

## 2. settings.html — Fix addMount() stub

- [x] 2.1 Replace the `addMount()` stub in `settings.html` with `await invoke('open_wizard')` wrapped in a try/catch that calls `showStatus(e.toString(), 'error')` on failure (relies on the `showStatus` helper added by `fix-ui-feedback`)
- [ ] 2.2 Verify that clicking "Add Mount" in the Mounts tab opens the wizard (or focuses it if already open) and leaves the settings window open in the background on all three platforms

## 3. wizard.html — step-source: real OneDrive path

- [x] 3.1 Rewrite `selectSource('drive')` in `wizard.html`: call `invoke('list_mounts')`, find the first mount with `mount_type === 'drive'` to get its `drive_id`, derive a new mount point label (e.g., "OneDrive 2") to avoid collision, call `invoke('add_mount', { mount_type: 'drive', drive_id, mount_point })`, then on success call `showStep('step-done')` and refresh the mount list
- [x] 3.2 If no drive-type mount is found in `list_mounts`, display an inline error "OneDrive is not yet available — please wait a moment and try again" in `step-source` without navigating away
- [x] 3.3 On `add_mount` rejection for the OneDrive path, display the error inline in `step-source`

## 4. wizard.html — step-sharepoint DOM and CSS

- [x] 4.1 Add a `<div id="step-sharepoint" class="step">` block to wizard.html containing: a "Back" link (`onclick="showStep('step-source')"`), an `<h1>Select SharePoint Site</h1>`, a search input (`id="sp-search"`) with a Search button (`onclick="searchSites()"`), an empty-state/error div (`id="sp-error"`), a site results list (`id="sp-sites"`), a library results section (`id="sp-libraries"` — hidden initially) with heading "Select Library" and a list (`id="sp-lib-list"`), and a loading spinner (`id="sp-spinner"` — hidden initially)
- [x] 4.2 Add CSS for `.sp-result-row` (clickable result rows with hover state, left-aligned, white background, border, padding, cursor pointer) to match the existing wizard aesthetic
- [x] 4.3 Add a "Back to sites" link inside `id="sp-libraries"` that hides the library section and shows the site results again

## 5. wizard.html — step-sharepoint JS: site search

- [x] 5.1 Implement `selectSource('sharepoint')` to call `showStep('step-sharepoint')` and reset the step state (clear results, clear error, clear search input, hide library section)
- [x] 5.2 Implement `searchSites()`: read value from `sp-search`, show spinner, clear previous results and error, call `invoke('search_sites', { query })`, hide spinner, render each result as a `.sp-result-row` div showing `site.display_name` and `site.web_url` with `onclick="selectSite(site)"` (site object stored in closure)
- [x] 5.3 Handle empty `search_sites` result: display "No sites found — try a different search term" in `sp-error`
- [x] 5.4 Handle `search_sites` rejection: display the error string in `sp-error`
- [x] 5.5 Allow submitting the search with the Enter key on the `sp-search` input (keydown listener calling `searchSites()` on Enter)

## 6. wizard.html — step-sharepoint JS: library selection and mount

- [x] 6.1 Implement `selectSite(site)`: store `site` in a module-level `selectedSite` variable, show spinner, clear library list and error, call `invoke('list_drives', { site_id: site.id })`, hide spinner, then branch: if exactly one library call `confirmMount(site, libraries[0])` directly; if multiple render each as a `.sp-result-row` in `sp-lib-list` with `onclick="confirmMount(selectedSite, lib)"` and show the `sp-libraries` section
- [x] 6.2 Handle `list_drives` rejection: display error in `sp-error`, leave site results visible
- [x] 6.3 Implement `confirmMount(site, library)`: show spinner, derive mount point as `'~/Cloud/' + site.display_name + ' - ' + library.name + '/'`, call `invoke('add_mount', { mount_type: 'sharepoint', mount_point, drive_id: library.id, site_id: site.id, site_name: site.display_name, library_name: library.name })`, hide spinner, on success call `refreshAndFinish()`, on error display error in `sp-error`
- [x] 6.4 Implement `refreshAndFinish()`: call `invoke('list_mounts')`, re-render the mount list in `step-done`, then call `showStep('step-done')`

## 7. wizard.html — step-done mount list rendering

- [x] 7.1 Extract the mount-list rendering logic in `init()` into a reusable `renderMountList(mounts)` function that populates a `<ul id="done-mount-list">` element in `step-done` (add that element to the `step-done` DOM if not already present) — each item shows `m.name + ' → ' + m.mount_point`
- [x] 7.2 Update `onSignInComplete()` to also call `renderMountList(mounts)` before `showStep('step-done')` so the list is populated on first-run completion
- [x] 7.3 Verify `refreshAndFinish()` (task 6.4) calls `renderMountList` so newly added mounts appear in `step-done`

## 8. Manual verification

- [ ] 8.1 First-run flow: sign in → `step-source` appears → click "SharePoint Site" → search → select site → select library → `step-done` shows new mount
- [ ] 8.2 First-run flow: sign in → `step-source` appears → click "OneDrive" → `step-done` shows OneDrive mount (or error if already present edge case)
- [ ] 8.3 Settings "Add Mount" → wizard opens (or focuses) at `step-source`; close wizard → settings still open
- [ ] 8.4 Tray "Add Mount" → wizard opens at `step-source` (existing behavior, regression check)
- [ ] 8.5 Single-library site: selecting a site with one library skips library list and goes directly to mount confirmation
- [ ] 8.6 Error states: invalid search (network off) shows inline error; `list_drives` failure shows inline error; `add_mount` failure shows inline error — all recoverable without reloading the page
