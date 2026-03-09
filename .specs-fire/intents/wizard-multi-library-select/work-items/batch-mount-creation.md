---
id: batch-mount-creation
title: Batch mount creation and feedback
intent: wizard-multi-library-select
complexity: medium
mode: confirm
status: completed
depends_on:
  - multi-select-library-ui
created: 2026-03-09T10:00:00Z
run_id: run-cloud-mount-010
completed_at: 2026-03-09T16:25:13.589Z
---

# Work Item: Batch mount creation and feedback

## Description

Wire the "Add selected" button to iterate through selected libraries, call `add_mount` for each, and provide clear progress and result feedback. After all mounts are created, update the library list to reflect newly-mounted state and refresh the "Added" section. Handle partial failures gracefully — if some mounts succeed and others fail, show which succeeded and which failed with actionable error messages.

## Acceptance Criteria

- [ ] Clicking "Add selected" calls `add_mount` for each selected library sequentially
- [ ] A loading/progress indicator is shown during the batch operation
- [ ] On full success: all newly-added libraries transition to "already mounted" state in the list
- [ ] On full success: the "Added" section (#sources-added-section) is updated with new entries
- [ ] On partial failure: succeeded mounts are reflected, failed ones show error message
- [ ] On full failure: error feedback shown via showStatus() or inline message
- [ ] The "Add selected" button is disabled during the operation to prevent double-submit
- [ ] Selection state is cleared after successful mount creation
- [ ] Mount point derivation follows existing pattern: ~/Cloud/{site_name} - {library_name}/

## Technical Notes

Key files: `wizard.js` (new batch mount function), `ui.js` (showStatus for feedback). Each `add_mount` call is independent — use a sequential loop (not Promise.all) to avoid race conditions on config file writes. After the loop, re-fetch mounts to update the mounted set.

## Dependencies

- multi-select-library-ui
