# Implementation Plan — run-cloud-mount-011 (wide)

## Work Item: fix-cfapi-double-sanitize

### Approach
Remove the caller-side `drive_id.replace('!', "_")` at main.rs:798 so `build_sync_root_id` in cfapi.rs is the single sanitization owner. The replacement is idempotent, so behavior is unchanged.

### Files to Modify
- `crates/cloudmount-app/src/main.rs` — line 798: change `drive_id.replace('!', "_")` to `drive_id.to_string()`

### Tests
- `cargo check` and `cargo clippy` pass clean

---

## Work Item: fix-settings-error-feedback

### Approach
1. S4/S5: Add `showStatus()` calls to `loadSettings` and `loadMounts` catch blocks
2. M1/M2/M3: Replace fragile querySelector chains with getElementById for buttons that already have IDs

### Files to Modify
- `crates/cloudmount-app/dist/settings.js` — 5 changes (2 error feedback, 3 selector fixes)

---

## Work Item: fix-wizard-error-feedback

### Approach
6 fixes in `wizard.js`, all using `showStatus()` from `ui.js`:

1. **B1** (line 42): `startSignIn` catch — add `showStatus('Sign-in failed', 'error')` before returning to welcome step
2. **S1** (line 367-374): `removeMount` in `addSourceEntry` — restructure so DOM row is only removed on success; on catch show `showStatus()` error
3. **S2** (line 416-418): `complete_wizard` catch — add `showStatus('Failed to complete setup', 'error')` and `return` to prevent proceeding to success step
4. **S3** (line 420-428): `list_mounts` on success step — wrap in try/catch with fallback (show step anyway, just skip mount list rendering)
5. **S6** (line 66-68): `copyAuthUrl` clipboard failure — add `showStatus('Could not copy URL', 'error')`
6. **D1** (line 393+): `getStarted` — disable button, show "Setting up…" during async, restore on error

### Files to Modify
- `crates/cloudmount-app/dist/wizard.js` — 6 changes across startSignIn, addSourceEntry, getStarted, copyAuthUrl

---

## Work Item: add-accessibility-support

### Approach
5 accessibility fixes across 5 files:

1. **A1**: Add `for` attributes to labels in settings.html (sync-interval, cache-dir, cache-max-size, metadata-ttl, log-level)
2. **A2**: Tab ARIA roles in settings.html (role="tablist", role="tab" with tabindex/aria-selected/aria-controls/id, role="tabpanel" with aria-labelledby) + keyboard navigation in settings.js (ArrowLeft/Right/Home/End/Enter/Space)
3. **A3**: Add `role="alert"` to error divs in wizard.html (#auth-error, #sources-sp-error, #sources-error)
4. **A4**: Add `role="status" aria-live="polite"` to #status-bar in both HTML files
5. **A5**: Add role="button"/tabindex="0"/keydown (Enter/Space) to .sp-result-row; add role="checkbox"/aria-checked/tabindex="0"/keydown to .sp-lib-row in wizard.js. Clean up role/tabindex on rows transitioned to mounted state. Add aria-hidden to visual checkmark.

### Files to Modify
- `crates/cloudmount-app/dist/settings.html` — A1, A2, A4
- `crates/cloudmount-app/dist/wizard.html` — A3, A4
- `crates/cloudmount-app/dist/settings.js` — A2 keyboard nav
- `crates/cloudmount-app/dist/wizard.js` — A5 interactive rows
- `crates/cloudmount-app/dist/styles.css` — focus-visible styles for tabs, rows

---

## Work Item: ux-polish

### Approach
5 minor UX fixes:

1. **D3**: Add dismiss button (×) to error status bar in `ui.js`. Button uses addEventListener, styled via `.status-dismiss` in CSS.
2. **M4**: Cache followed sites in `cachedFollowedSites` variable in wizard.js. Restore from cache when search is cleared instead of re-fetching.
3. **M5**: Show "Previous selections cleared" info notification in `selectSiteInSources()` when switching sites with active selections.
4. **M6**: Show "No mounts configured" empty state in `loadMounts()` when mount list is empty.
5. **M7**: Update `document.title` per wizard step via `_stepTitles` map in `showStep()`.

### Files to Modify
- `crates/cloudmount-app/dist/ui.js` — D3 (dismiss button)
- `crates/cloudmount-app/dist/styles.css` — D3 (`.status-dismiss`), M6 (`.mount-empty`)
- `crates/cloudmount-app/dist/wizard.js` — M4 (cache), M5 (notification), M7 (title)
- `crates/cloudmount-app/dist/settings.js` — M6 (empty state)
