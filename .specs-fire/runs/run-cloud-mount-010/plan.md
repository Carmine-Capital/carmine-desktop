---
run: run-cloud-mount-010
work_item: batch-mount-creation
intent: wizard-multi-library-select
mode: confirm
generated: 2026-03-09T16:20:00Z
---

# Implementation Plan: Batch mount creation and feedback

## Approach

The core batch mount loop already exists in `confirmSelectedLibraries()` from work item `multi-select-library-ui` (run-009). This work item adds **progress feedback**, **success confirmation**, and **improved error handling** to that existing function. Changes are minimal and surgical — all within the existing function body plus one HTML element addition.

## Current State

`confirmSelectedLibraries()` (wizard.js:282-337) already:
- Loops through selected libraries, calling `add_mount` sequentially
- Disables the "Add selected" button during operation
- Transitions rows to mounted state on success
- Adds entries to the "Added" section
- Shows error text for partial failures
- Clears selection after operation

## Gaps vs Acceptance Criteria

| Criterion | Status | Gap |
|-----------|--------|-----|
| Progress indicator during batch | Spinner only | No per-item progress text |
| Full success feedback | Silent | No success message |
| Full failure feedback | Same as partial | No distinct full-failure message |
| Selection cleared after **successful** mount | Always cleared | Should preserve on full failure for retry |

## Files to Create

(none)

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Add `<div id="status-bar"></div>` before `</body>` for `showStatus()` support |
| `crates/cloudmount-app/dist/wizard.js` | Enhance `confirmSelectedLibraries()`: button progress text, success/failure feedback via `showStatus()`, preserve selection on full failure |

## Detailed Changes

### wizard.html
- Add `<div id="status-bar"></div>` just before `</body>` (same pattern as settings.html:82)

### wizard.js — `confirmSelectedLibraries()`
1. **Progress text on button**: Update button text to `Adding 1 of N...` as each `add_mount` is called
2. **Success path**: When all succeed → `showStatus('N libraries added successfully', 'success')`, clear selection
3. **Partial failure path**: Some succeed, some fail → Show inline error listing failed library names, clear succeeded items from selection but keep failed ones
4. **Full failure path**: All fail → `showStatus('Failed to add libraries — check your connection', 'error')`, preserve entire selection for retry
5. **Button text restore**: Reset button text via `updateAddSelectedBtn()` after operation

## Tests

- `cargo test -p cloudmount-app`: Verify no Rust compilation regressions
- `cargo clippy --all-targets --all-features`: Zero warnings
- Manual verification per acceptance criteria (frontend-only changes, no unit test framework for vanilla JS)

## Risk Assessment

Low — No backend changes, no new dependencies, no structural changes. All modifications are within a single existing function plus one HTML element.
