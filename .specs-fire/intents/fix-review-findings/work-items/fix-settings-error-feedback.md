---
id: fix-settings-error-feedback
title: Add error feedback and fix fragile selectors in settings
intent: fix-review-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-09T17:00:00Z
run_id: run-cloud-mount-011
completed_at: 2026-03-09T17:10:19.312Z
---

# Work Item: Add error feedback and fix fragile selectors in settings

## Description

Fix 5 issues in settings.js:

- **S4**: `loadSettings` catch only does console.error — add showStatus('Failed to load settings', 'error')
- **S5**: `loadMounts` catch only does console.error — add showStatus('Failed to load mounts', 'error')
- **M1**: `saveAdvanced` uses fragile `querySelector('#advanced .actions button')` — use getElementById('btn-save-advanced')
- **M2**: `clearCache` uses fragile `querySelector('#advanced .actions .btn-danger')` — use getElementById('btn-clear-cache')
- **M3**: `signOut` uses fragile `querySelector('#account button')` — use getElementById('btn-sign-out')

## Acceptance Criteria

- [ ] loadSettings failure shows error status to user
- [ ] loadMounts failure shows error status to user
- [ ] saveAdvanced, clearCache, signOut use getElementById instead of fragile querySelector chains
- [ ] Button IDs exist in settings.html (verify or add if missing)
- [ ] All errors use showStatus() from ui.js

## Technical Notes

All changes in `crates/cloudmount-app/dist/settings.js` and potentially `settings.html` if button IDs need adding. Verify that the button elements already have the expected IDs before switching selectors.

## Dependencies

(none)
