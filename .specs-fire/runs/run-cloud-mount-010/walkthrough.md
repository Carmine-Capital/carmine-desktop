---
run: run-cloud-mount-010
work_item: batch-mount-creation
intent: wizard-multi-library-select
generated: 2026-03-09T16:25:00Z
mode: confirm
---

# Implementation Walkthrough: Batch mount creation and feedback

## Summary

Enhanced the wizard's batch mount confirmation flow with per-item progress feedback on the button, three-way result messaging (full success / partial failure / full failure), and selective selection cleanup that preserves failed items for retry. Also added the `status-bar` element to wizard.html to enable `showStatus()` toast notifications.

## Structure Overview

The change builds on the existing `confirmSelectedLibraries()` function from run-009. The function already looped through selected libraries calling `add_mount` sequentially with row transitions and error collection. This work item adds a feedback layer: the "Add selected" button doubles as a progress indicator during the operation, `showStatus()` toasts provide result feedback, and the selection Map is cleaned up selectively rather than wholesale — failed items stay selected so the user can retry without re-picking.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Added `<div id="status-bar"></div>` before scripts for `showStatus()` support |
| `crates/cloudmount-app/dist/wizard.js` | Rewrote `confirmSelectedLibraries()` with progress text, three-way feedback, and selective cleanup |

## Key Implementation Details

### 1. Button as progress indicator

Instead of relying on the `sources-sp-spinner` (which is also used for site search loading), the "Add selected" button text is updated on each iteration to "Adding 1 of 3..." / "Adding 2 of 3..." etc. After the loop, `updateAddSelectedBtn()` restores the proper button state — either hidden (all succeeded) or showing the remaining count (partial failure).

### 2. Three-way result feedback

The result handling now distinguishes three cases:
- **Full success** (0 errors): Green toast via `showStatus('N libraries added successfully', 'success')`
- **Partial failure** (some errors, some successes): Inline error listing failed library names + info toast summarizing counts
- **Full failure** (all errors): Red toast via `showStatus('Failed to add libraries — check your connection', 'error')`

### 3. Selective selection cleanup

Changed from `selectedLibraries.clear()` to iterating only the `succeeded` array and calling `selectedLibraries.delete(driveId)` for each. On full failure, no items are deleted — the user's selection is intact for retry. On partial failure, only succeeded items are removed, leaving failed items selected.

### 4. Spinner removal

Removed the `sources-sp-spinner` show/hide from this function. The button text progress is a more direct and less ambiguous progress indicator than an unassociated spinner element shared with the site search flow.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Progress location | Button text | Direct association with the action; avoids confusing shared spinner |
| Feedback mechanism | `showStatus()` toasts | Consistent with settings.html pattern; non-blocking, auto-dismissing |
| Singular vs plural | Conditional message | "Library added" for 1, "N libraries added" for N>1 — natural language |
| Selection on failure | Preserve failed items | Enables retry without re-navigating and re-selecting |

## Deviations from Plan

None. Implementation matches the approved plan exactly.

## Dependencies Added

(none)

## How to Verify

1. **Build and run the app**
   ```bash
   toolbox run -c cloudmount-build cargo run -p cloudmount-app --features desktop
   ```

2. **Test full success path**
   Sign in → Navigate to a SharePoint site → Select 2+ libraries → Click "Add selected (2)"
   - Button should show "Adding 1 of 2..." then "Adding 2 of 2..."
   - Green toast: "2 libraries added successfully"
   - Both rows transition to mounted state with "Already added" badge
   - Both appear in "Added" section
   - Selection is cleared, button is hidden

3. **Test progress indicator**
   Select multiple libraries → Watch the button text during operation
   - Should update on each item: "Adding 1 of N...", "Adding 2 of N...", etc.

4. **Test full failure path** (simulate by disconnecting network)
   Select libraries → Disconnect network → Click "Add selected"
   - Red toast: "Failed to add libraries — check your connection"
   - Selection remains intact for retry
   - Button reappears with count showing

5. **Test partial failure path** (if reproducible)
   - Info toast showing "X added, Y failed"
   - Inline error listing specific failed library names
   - Succeeded rows transition to mounted; failed rows stay selected

## Test Coverage

- Tests run: 131 passed, 0 failed, 15 ignored
- Coverage: N/A (vanilla JS frontend, no unit test framework)
- Clippy: 0 warnings

## Developer Notes

- The `status-bar` div must appear BEFORE the script tags in wizard.html so that `showStatus()` from ui.js can find it when called.
- `updateAddSelectedBtn()` handles all button state restoration: if `selectedLibraries` is empty it hides the button; if items remain (partial failure) it shows the count. No manual button text reset needed.
- The `renderFollowedSites()` function still uses `.onclick` assignment (pre-existing pattern from before run-009). Standardizing to `addEventListener` would be a separate cleanup task.

---
*Generated by specs.md FIRE Flow Run run-cloud-mount-010*
