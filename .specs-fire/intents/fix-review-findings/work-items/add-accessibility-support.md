---
id: add-accessibility-support
title: Add ARIA attributes and keyboard navigation
intent: fix-review-findings
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T17:00:00Z
run_id: run-cloud-mount-011
completed_at: 2026-03-09T17:27:56.986Z
---

# Work Item: Add ARIA attributes and keyboard navigation

## Description

Fix 5 accessibility issues across settings and wizard:

- **A1**: Form inputs in settings.html lack `<label for="...">` associations — add `for` attributes to labels for sync-interval, cache-dir, cache-max-size, metadata-ttl, log-level
- **A2**: Tab elements in settings.html are `<div class="tab">` with no keyboard support — add tabindex="0", role="tab", aria-selected, aria-controls; add role="tablist" to container, role="tabpanel" to panels; handle arrow key navigation in settings.js
- **A3**: Error messages in wizard.html (#auth-error, #sources-error, #sources-sp-error) lack role="alert" — add the attribute
- **A4**: #status-bar in both HTML files has no ARIA live region — add role="status" or aria-live="polite"
- **A5**: .sp-result-row and .sp-lib-row in wizard are non-semantic click targets — add role="button", tabindex="0", and keydown handler for Enter/Space in wizard.js

## Acceptance Criteria

- [ ] All form inputs in settings.html have associated labels via `for` attribute
- [ ] Settings tabs are keyboard-navigable with arrow keys and have proper ARIA tab roles
- [ ] Error divs in wizard.html have role="alert"
- [ ] Status bar in both HTML files has aria-live="polite" or role="status"
- [ ] Interactive card elements (.sp-result-row, .sp-lib-row) are focusable and activatable via keyboard
- [ ] No inline event handlers (CSP compliant)

## Technical Notes

HTML changes in `settings.html` and `wizard.html`. JS changes in `settings.js` (tab keyboard nav) and `wizard.js` (card keyboard activation). For tab navigation, listen for ArrowLeft/ArrowRight keydown on tabs and move focus + activate.

## Dependencies

(none)
