---
run: run-cloud-mount-002
work_item: wizard-add-mount-mode-ui
intent: fix-wizard-add-mount-ux
generated: 2026-03-08T11:35:00Z
mode: autopilot
---

# Implementation Walkthrough: Replace "Get started" with "Close" in add-mount wizard mode

## Summary

The wizard now distinguishes between two entry paths: initial setup (first-time sign-in) and add-mount mode (tray → "Add Mount…" on an already-authenticated account). In add-mount mode, the OneDrive section is hidden (it is already mounted) and the action button becomes an always-enabled "Close" button that dismisses the window. The initial setup flow is completely unchanged.

## Structure Overview

A single module-level boolean flag (`addMountMode`) is the sole source of truth for which mode the wizard is in. Three functions gate on this flag: `loadSources()` applies it after the data fetch to reconfigure the UI; `updateGetStartedBtn()` returns early so the checkbox state cannot override the button; and the `get-started-btn` click handler routes to either `close()` or `getStarted()` depending on the flag. The flag is set exactly once, in `goToAddMount()`, which is the function tray.rs invokes via `win.eval`.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.js` | Added `addMountMode` flag; set in `goToAddMount()`; add-mount UI block in `loadSources()`; early return guard in `updateGetStartedBtn()`; mode-aware click handler for `get-started-btn` |

## Key Implementation Details

### 1. Flag placement and lifetime

`addMountMode` lives at module scope alongside other wizard state (`signingIn`, `onedriveDriveId`, etc.). It starts `false`. The page is created fresh each time the wizard window opens, so there is no risk of stale state across sessions. When tray.rs calls `goToAddMount()` the flag flips to `true` before any async work begins.

### 2. OneDrive section suppression in `loadSources()`

`loadSources()` hides all sections at the start of each load, then conditionally shows the OneDrive section if `get_drive_info` succeeds. The add-mount block runs immediately after: it unconditionally hides that section again and reconfigures the button. Both display assignments happen synchronously in the same post-`await` block, so the browser batches them into a single repaint — no visible flicker.

### 3. `updateGetStartedBtn()` guard

The function is called at the end of `loadSources()` and also by `mountLibraryInSources()` and the remove button handler. The early return prevents any of these call sites from accidentally re-disabling the "Close" button in add-mount mode.

### 4. Click handler routing

The `get-started-btn` listener was previously wired directly to `getStarted`. It is now wrapped in an arrow function that checks `addMountMode` first. The Tauri `close()` call is the same API used by the existing "Close" button in `step-success`, maintaining consistency.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Where to set the flag | Inside `goToAddMount()` | tray.rs calls this function via `win.eval` — it is the natural entry point for add-mount mode |
| Where to apply UI changes | Inside `loadSources()` after drive block | Data must be fetched first (to capture `onedriveDriveId`); UI override is applied synchronously immediately after |
| Button label change via JS | `btn.textContent` | No HTML structural change needed; JS owns all mode-specific state |
| Close mechanism | `window.__TAURI__.window.getCurrentWindow().close()` | Same pattern as the existing `wizard-close-btn` handler |

## Deviations from Plan

None. Implementation matches the work item specification exactly.

## Dependencies Added

(none)

## How to Verify

1. **Initial setup path (must be unaffected)**
   - Sign out if authenticated
   - Open CloudMount (wizard shows automatically or via tray)
   - Verify: "Get started" button is present and disabled until OneDrive checkbox is checked
   - Check OneDrive checkbox → button enables → click → mounts OneDrive → shows success step

2. **Add-mount mode via tray**
   - Ensure signed in and OneDrive already mounted
   - Tray → "Add Mount…"
   - Expected: wizard opens directly at sources step; OneDrive section is hidden; button reads "Close" and is enabled
   - Click "Close" → wizard window closes

3. **SharePoint in add-mount mode still works**
   - In add-mount mode, select a SharePoint site and library
   - Expected: library mounts immediately on click (via `mountLibraryInSources`); "Close" button remains enabled and unchanged

4. **Rust test suite**
   ```bash
   cargo test -p cloudmount-app
   ```
   Expected: `18 passed; 0 failed`

## Test Coverage

- Tests added: 0 (no JS test framework; logic verified via static analysis)
- Rust suite: 18 passed, 0 failed, 2 ignored (live API)
- All 9 acceptance criteria: PASS (see test-report.md)

## Developer Notes

- `onSourcesSpSearchInput()` calls `loadSources()` when the search field is cleared. In add-mount mode this is safe: the add-mount block re-runs on each `loadSources()` call and correctly keeps the OneDrive section hidden and the button set to "Close".
- `goToAddMount()` is also called from `init()` when `is_authenticated` returns true (line 365-368 in the final file). This means the flag is set before tray.rs makes its `win.eval("goToAddMount()")` call, which is a no-op re-entry into an already-correct state.
- There is no reset path for `addMountMode` — the window is destroyed and recreated each time, so the flag resets naturally.

---
*Generated by FIRE Builder Agent — run-cloud-mount-002*
