---
id: fix-wizard-error-feedback
title: Add user-visible error feedback to wizard silent failures
intent: fix-review-findings
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T17:00:00Z
run_id: run-cloud-mount-011
completed_at: 2026-03-09T17:19:20.360Z
---

# Work Item: Add user-visible error feedback to wizard silent failures

## Description

Fix 6 silent/blocked error paths in wizard.js and add loading state to the "Get started" button:

- **B1**: `startSignIn` catch block only does console.error — show error via showStatus() before returning to welcome step
- **S1**: `removeMount` catch swallows error — show error and don't remove DOM row on failure
- **S2**: `complete_wizard` catch swallows error — log and show status on failure
- **S3**: `list_mounts` on success step has no try/catch — wrap with error handling
- **S6**: `copyAuthUrl` clipboard failure silent — show error via showStatus()
- **D1**: "Get started" button has no loading/disabled state — disable and show "Setting up..." during async work, restore on error

## Acceptance Criteria

- [ ] Sign-in failure shows user-visible error message before returning to welcome step
- [ ] removeMount failure shows error and preserves DOM row
- [ ] complete_wizard failure shows error status
- [ ] list_mounts on success step wrapped in try/catch with error feedback
- [ ] Clipboard copy failure shows error status
- [ ] "Get started" button is disabled with "Setting up..." text during async operations
- [ ] No inline event handlers (CSP compliant)
- [ ] All errors use showStatus() from ui.js

## Technical Notes

All changes in `crates/cloudmount-app/dist/wizard.js`. Use `showStatus(message, 'error')` for error feedback. The "Get started" button should be re-enabled with original text on error.

## Dependencies

(none)
