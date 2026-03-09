# Implementation Plan — multi-select-library-ui

**Run**: run-cloud-mount-009
**Work Item**: multi-select-library-ui
**Mode**: confirm
**Intent**: wizard-multi-library-select

## Approach

Replace the current single-click-to-mount library rows in `selectSiteInSources()` with checkbox-style multi-select rows. On site selection, fetch current mounts via `list_mounts` and cross-reference by `drive_id` to grey out already-mounted libraries. A floating "Add selected (N)" button appears when >= 1 library is checked. Clicking it loops over selected libraries, calls `add_mount` for each, transitions them to "already mounted" state, and stays on the library list.

## Files to Modify

1. **`crates/cloudmount-app/dist/wizard.js`** — Multi-select logic, already-mounted detection
2. **`crates/cloudmount-app/dist/wizard.html`** — "Add selected" button element
3. **`crates/cloudmount-app/dist/styles.css`** — Library row states (selected, mounted)

## Files to Create

(none)

## Tests

Manual validation + existing `cargo test` for backend integrity.

## Decisions

- No backend changes — `list_mounts` already returns `drive_id`
- Selection tracked in a `Set` (O(1) operations)
- Row click toggles selection with visual check indicator via CSS
