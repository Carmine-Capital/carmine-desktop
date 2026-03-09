---
run: run-cloud-mount-009
work_item: multi-select-library-ui
intent: wizard-multi-library-select
generated: 2026-03-09T16:16:00Z
mode: confirm
---

# Implementation Walkthrough: Multi-select library UI with already-mounted detection

## Summary

Replaced the wizard's single-click-to-mount SharePoint library flow with a checkbox-style multi-select interface. Libraries are now listed with visual check indicators. Already-mounted libraries are detected by cross-referencing `list_mounts` drive IDs and rendered greyed out with an "Already added" badge. Users select multiple libraries, then confirm with a single "Add selected (N)" button. After confirmation, the wizard stays on the library list and newly mounted libraries transition to the mounted visual state.

## Structure Overview

The change is entirely frontend (vanilla JS + CSS). When a user clicks a SharePoint site, `selectSiteInSources()` now fetches both the site's libraries and the current mount list in parallel. It builds a Set of already-mounted drive IDs, then renders each library as a row with a checkbox indicator. Non-mounted rows get click listeners that toggle selection in a Map. A floating "Add selected" button appears when any libraries are checked. On confirm, the system loops through selected libraries, calls the existing `add_mount` Tauri command for each, and transitions the corresponding row to mounted state in-place — no site-list reset.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Added `#add-selected-btn` button inside `#sources-sp-libraries` container |
| `crates/cloudmount-app/dist/wizard.js` | Added `selectedLibraries` Map, rewrote `selectSiteInSources()` with multi-select + mounted detection, added `updateAddSelectedBtn()` and `confirmSelectedLibraries()`, removed `mountLibraryInSources()`, wired event listeners for new button and back-button cleanup |
| `crates/cloudmount-app/dist/styles.css` | Added `.sp-lib-row` (base, selected, mounted states), `.lib-check` checkbox indicator, `.lib-info`/`.lib-name`/`.lib-badge` layout, `#add-selected-btn` full-width button |

## Key Implementation Details

### 1. Already-mounted detection

On site selection, `list_mounts` and `list_drives` are fetched in parallel via `Promise.all`. A `Set` of mounted `drive_id` values is built from the mounts response. Each library row checks membership to determine if it should render as mounted (greyed, green checkmark, "Already added" badge, no click listener).

### 2. Selection state management

A module-level `Map<driveId, {site, library}>` tracks selected libraries. Using a Map (not a Set) preserves the site and library objects needed for the `add_mount` call. The Map is cleared on site navigation, back-button press, and after confirmation.

### 3. Row-to-mounted transition

After a successful `add_mount`, each row is transitioned in-place: the `selected` class is removed, `mounted` class added, and `replaceWith(cloneNode(true))` removes the click listener. An "Already added" badge is appended to the re-queried element. `CSS.escape()` is used for safe attribute selectors.

### 4. Partial failure handling

If some libraries in a batch fail to mount, errors are collected and displayed in aggregate. Successfully mounted libraries still transition to mounted state.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Selection data structure | `Map<driveId, {site, library}>` | Need both site and library metadata for `add_mount` call, Map provides O(1) lookup by drive ID |
| Visual checkbox | CSS div with checkmark glyph | Cleaner dark-theme aesthetic than native `<input type="checkbox">`, consistent with design system |
| Listener removal | `replaceWith(cloneNode(true))` | Simple, idiomatic way to strip all event listeners from a DOM element |
| No backend changes | Reuse existing `add_mount` and `list_mounts` commands | Backend already has all needed data (`drive_id` in `MountInfo`), frontend loops over selections |

## Deviations from Plan

None. Implementation matches the plan exactly.

## Dependencies Added

(none)

## How to Verify

1. **Build and run the app**
   ```bash
   toolbox run -c cloudmount-build cargo run -p cloudmount-app --features desktop
   ```

2. **Navigate to SharePoint library selection**
   Sign in → Click a SharePoint site → Observe library list

3. **Verify multi-select behavior**
   - Click a library row → row highlights with purple border and filled checkbox
   - Click again → deselects
   - "Add selected (N)" button appears when >= 1 selected, hidden when 0
   - Already-mounted libraries appear greyed with green check and "Already added" badge
   - Already-mounted libraries do not respond to clicks

4. **Verify batch confirmation**
   - Select 2+ libraries → Click "Add selected (2)"
   - Both mount successfully → rows transition to mounted state
   - Wizard stays on library list (does not reset to site list)
   - Both appear in "Added" section below

5. **Verify back button**
   - Click "Back to sites" → selection is cleared
   - Return to same site → fresh selection state

## Test Coverage

- Tests run: 131 passed, 0 failed, 15 ignored
- Coverage: N/A (vanilla JS frontend, no unit test framework)
- Status: All passing
- Clippy: 0 warnings

## Developer Notes

- `renderFollowedSites()` uses `.onclick` assignment (not `addEventListener`). This is existing code outside the scope of this change. A future cleanup could standardize all handlers to `addEventListener`.
- The `sources-sp-sites` div is NOT hidden when libraries are shown — both are visible simultaneously. This is the pre-existing UX pattern. The "Back to sites" button hides the libraries section.
- `CSS.escape()` is used when building `querySelector` strings with dynamic drive IDs. This prevents breakage if a drive ID contains special characters.

---
*Generated by specs.md FIRE Flow Run run-cloud-mount-009*
