---
run: run-cloud-mount-017
work_item: fix-frontend-errors
intent: fix-comprehensive-review
mode: confirm
checkpoint: plan
approved_at: pending
---

# Implementation Plan: Error handling — friendly messages, init catch, loading states, validation

## Approach

Address all 8 frontend error-handling issues in a single pass across 3 files (`ui.js`, `wizard.js`, `settings.js`). Changes are small, isolated, and additive — no structural refactoring.

1. **`ui.js`** — Add `formatError(e)` helper that maps raw Rust error strings to user-friendly messages. All `showStatus(e, 'error')` calls across both files will route through it.

2. **`wizard.js`** — 4 fixes:
   - Wrap `init()` body in try/catch → show error status on failure
   - Add disabled/loading state to remove button in `addSourceEntry()`
   - Show info message when one source loads but the other fails in `loadSources()`
   - Log warning in `cancelSignIn()` catch instead of silently swallowing

3. **`settings.js`** — 4 fixes:
   - Route all `showStatus(e, 'error')` calls through `formatError(e)`
   - Validate `parseInt()` results before IPC calls in `saveGeneral()` and `saveAdvanced()`
   - Re-enable sign-out button in success path
   - Add `.catch()` to top-level `loadSettings()` and `loadMounts()` calls

## Files to Create

| File | Purpose |
|------|---------|
| (none) | |

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/ui.js` | Add `formatError(e)` helper that maps Rust error patterns to friendly messages |
| `crates/cloudmount-app/dist/wizard.js` | Wrap init() in try/catch; add remove-button loading state; show partial source-load info; log cancelSignIn warning |
| `crates/cloudmount-app/dist/settings.js` | Use formatError(); validate parseInt; re-enable sign-out btn on success; add .catch() to top-level calls |

## Technical Details

### `formatError(e)` design

```js
function formatError(e) {
  const msg = (e instanceof Error) ? e.message : String(e);
  const patterns = [
    [/GraphApi\s*\{?\s*status:\s*401/i, 'Sign-in expired. Please re-authenticate.'],
    [/GraphApi\s*\{?\s*status:\s*403/i, 'Access denied. Check your permissions.'],
    [/GraphApi\s*\{?\s*status:\s*404/i, 'Resource not found. It may have been deleted.'],
    [/GraphApi\s*\{?\s*status:\s*429/i, 'Too many requests. Please wait a moment.'],
    [/GraphApi\s*\{?\s*status:\s*5\d\d/i, 'Server error. Please try again later.'],
    [/network|fetch|connect|timeout/i, 'Network error. Check your internet connection.'],
    [/token|auth|credential/i, 'Authentication error. Try signing in again.'],
  ];
  for (const [re, friendly] of patterns) {
    if (re.test(msg)) return friendly;
  }
  // Strip Rust enum wrappers if nothing matched
  return msg.replace(/^\w+\s*\{[^}]*message:\s*"([^"]+)".*\}$/, '$1');
}
```

Exported via `window.formatError = formatError` (same pattern as `showStatus`).

### parseInt validation pattern

```js
const val = parseInt(el.value);
if (isNaN(val) || val <= 0) {
  showStatus('Invalid number', 'error');
  return;
}
```

---
*Plan approved at checkpoint. Execution follows.*
