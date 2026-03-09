---
id: multi-select-library-ui
title: Multi-select library UI with already-mounted detection
intent: wizard-multi-library-select
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T10:00:00Z
run_id: run-cloud-mount-009
completed_at: 2026-03-09T16:15:57.392Z
---

# Work Item: Multi-select library UI with already-mounted detection

## Description

Replace the current single-click-to-mount library rows with checkbox-style multi-select rows. When loading libraries for a site, fetch current mounts via `list_mounts` and cross-reference by `drive_id` to identify already-mounted libraries. Already-mounted libraries appear greyed out with a check indicator and are non-selectable. A new "Add selected (N)" confirmation button appears when at least one library is checked. The wizard stays on the library list after confirming — no forced reset to site list.

## Acceptance Criteria

- [ ] Library rows display a checkbox or toggle indicator for selection
- [ ] Clicking a library row toggles its selected state (checked/unchecked)
- [ ] Already-mounted libraries are visually distinguished (greyed out, check mark, "Already added" label)
- [ ] Already-mounted libraries cannot be selected
- [ ] An "Add selected (N)" button appears when N >= 1 libraries are checked
- [ ] The button is hidden or disabled when no libraries are selected
- [ ] After confirming, the wizard remains on the library list (does not reset to site list)
- [ ] Newly added libraries transition to the "already mounted" visual state after confirmation
- [ ] Design is consistent with the dark premium design system
- [ ] No inline event handlers in HTML (addEventListener only)

## Technical Notes

Key files: `wizard.html` (library list container, new button), `wizard.js` (selectSiteInSources, new multi-select logic), `styles.css` (selected/disabled states). Use `invoke('list_mounts')` to get current mounts and compare `drive_id` fields. Track selected library IDs in a local Set or array in wizard.js.

## Dependencies

(none)
