---
id: fix-frontend-errors
title: Error handling — friendly messages, init catch, loading states, validation
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-017
completed_at: 2026-03-09T19:19:35.168Z
---

# Work Item: Error handling — friendly messages, init catch, loading states, validation

## Description

Fix frontend error handling and robustness:

1. **Raw Rust errors shown** (`settings.js`, `wizard.js`): `showStatus(e, 'error')` passes raw Rust `.to_string()` output. Fix: create `formatError(e)` helper in `ui.js` that maps common patterns to friendly messages (e.g., `"GraphApi { status: 401"` → `"Sign-in expired. Please re-authenticate."`).

2. **init() unhandled rejection** (`wizard.js:532`): If `invoke('is_authenticated')` throws, entire init fails silently — blank screen. Fix: wrap in try/catch, show `showStatus('Failed to initialize', 'error')`.

3. **Remove button double-click** (`wizard.js:408`): No disabled/loading state during async `remove_mount` call. Fix: disable button, set text to "Removing...", re-enable on error.

4. **parseInt without NaN validation** (`settings.js:117,137`): `parseInt()` can return NaN on empty/invalid input. Fix: validate before sending — `if (isNaN(val) || val <= 0) { showStatus('Invalid value', 'error'); return; }`.

5. **Sign-out button stuck** (`settings.js:198`): Button stays disabled on success (relies on Rust reload). Fix: re-enable in success path before showStatus.

6. **Promise.allSettled swallows errors** (`wizard.js:97-101`): If only one source fails, no indication. Fix: show info toast when one succeeds and one fails.

7. **Fire-and-forget async** (`settings.js:223-224`): Top-level `loadSettings()` and `loadMounts()` with no `.catch()`. Fix: add `.catch(e => showStatus('Failed to load', 'error'))`.

8. **cancelSignIn swallows errors** (`wizard.js:51`): `catch (_) {}` discards error. Fix: `console.warn('cancel failed:', e)`.

## Acceptance Criteria

- [ ] No raw Rust error strings shown to users — all mapped through formatError()
- [ ] wizard.js init() failure shows error status, not blank screen
- [ ] Remove button disabled during async operation, re-enabled on error
- [ ] parseInt results validated before IPC calls
- [ ] Sign-out button re-enabled in success path
- [ ] Partial source-load failure shows info message
- [ ] Top-level async calls have .catch() handlers
- [ ] cancelSignIn logs warning on failure

## Technical Notes

The `formatError()` helper in `ui.js` should handle: string errors (pass through), Error objects (.message), and Rust-formatted errors (regex match common patterns). Keep a map of pattern → friendly message.

## Dependencies

(none)
