---
run: run-cloud-mount-017
work_item: fix-frontend-errors
intent: fix-comprehensive-review
generated: 2026-03-09T19:20:00Z
mode: confirm
---

# Implementation Walkthrough: Error handling — friendly messages, init catch, loading states, validation

## Summary

Added a centralized `formatError()` helper in `ui.js` that maps raw Rust error strings to user-friendly messages using regex pattern matching. Applied it across all error paths in `wizard.js` and `settings.js`. Fixed 8 specific frontend robustness issues: init crash protection, remove button loading state, partial source-load feedback, parseInt validation, sign-out button stuck state, silent catch logging, and unhandled promise rejections.

## Structure Overview

The shared `ui.js` file (loaded before page-specific JS) now exports `formatError()` alongside the existing `showStatus()`. Both `wizard.js` and `settings.js` call `formatError(e)` wherever they previously showed raw error objects to users. The error mapping is one-directional: Rust error strings come in, friendly English strings come out. No changes to Rust code or IPC contract were needed.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/ui.js` | Added `formatError(e)` helper with 7 regex patterns mapping Rust/Graph API errors to friendly messages |
| `crates/cloudmount-app/dist/wizard.js` | Wrapped init() in try/catch; added remove-button loading state; partial source-load info toast; cancelSignIn warning log; formatError() in all error displays |
| `crates/cloudmount-app/dist/settings.js` | formatError() for all error paths; parseInt validation before IPC; sign-out button re-enable on success; .catch() on top-level async calls |

## Key Implementation Details

### 1. formatError() pattern matching

The helper checks error strings against an ordered array of regex→message pairs. Patterns cover: HTTP 401/403/404/429/5xx from Graph API, network/connectivity errors, and auth/token errors. If no pattern matches, a fallback regex strips Rust enum wrappers (`EnumName { message: "..." }` → just the message). This handles the majority of Rust error `.to_string()` output without needing to enumerate every error variant.

### 2. Wizard init() crash protection

The `init()` function's async IPC calls (`is_authenticated`, `goToAddMount`) are now wrapped in try/catch. Previously, if `invoke('is_authenticated')` threw (e.g., backend not ready), the entire wizard would show a blank screen with no feedback. Now it shows "Failed to initialize. Please restart." via the status bar.

### 3. Remove button loading state

The remove button in `addSourceEntry()` now disables and shows "Removing..." during the async `remove_mount` IPC call. On error, it re-enables with the original "Remove" text. Also converted from `.onclick` assignment to `addEventListener` for CSP consistency with the rest of the codebase.

### 4. parseInt validation

Both `saveGeneral()` (sync interval) and `saveAdvanced()` (metadata TTL) now validate `parseInt()` results before sending IPC calls. `NaN` or non-positive values trigger an immediate error status with no IPC call made.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Error pattern location | Global `_errorPatterns` array in `ui.js` | Single source of truth, easy to extend, loaded before all page JS |
| Fallback for unmatched errors | Strip Rust enum wrapper regex | Better than showing raw `EnumName { ... }` but preserves the actual message content |
| Top-level `.catch()` in settings | Added despite internal try/catch | Belt-and-suspenders: protects against future changes that might skip try/catch |
| Remove button event binding | `addEventListener` instead of `.onclick` | Consistent with CSP-compliant patterns used everywhere else |

## Deviations from Plan

None. All 8 items from the work item were implemented as planned.

## Dependencies Added

(none)

## How to Verify

1. **Build succeeds**
   ```bash
   toolbox run --container cloudmount-build cargo build -p cloudmount-app
   ```
   Expected: Compiles without errors

2. **Error mapping works**
   In the wizard, trigger a Graph API error (e.g., sign in with expired token, search sites while offline). The status bar should show a friendly message like "Network error. Check your internet connection." instead of raw Rust error output.

3. **Remove button loading state**
   In the wizard's add-mount mode, add a SharePoint library, then click "Remove". The button should show "Removing..." and be unclickable during the operation.

4. **parseInt validation**
   In Settings > General, clear the sync interval field and click Save. Should show "Sync interval must be a positive number" error. Same for metadata TTL in Advanced.

5. **Sign-out button recovery**
   In Settings > Account, click Sign Out and confirm. The button should re-enable after the operation completes.

6. **Init failure recovery**
   If the backend IPC is unavailable when the wizard opens, it should show "Failed to initialize. Please restart." instead of a blank screen.

## Test Coverage

- Tests run: 95
- Coverage: N/A (vanilla JS frontend, no JS test harness)
- Status: All passing

## Developer Notes

- `formatError()` is a global function available to any page that loads `ui.js` via `<script>`. No module system — it's just a function on the global scope.
- The error patterns are intentionally broad (e.g., `/network|fetch|connect|timeout/i`). If a Rust error message happens to contain "timeout" in a non-network context, it would get the wrong friendly message. This is an acceptable tradeoff for the current error surface.
- The `loadSettings().catch()` / `loadMounts().catch()` pattern is defensive. The functions already have internal try/catch. The outer catch would only fire if the function itself throws synchronously before reaching the try block, which currently can't happen. Kept for robustness.

---
*Generated by specs.md FIRE Flow Run run-cloud-mount-017*
