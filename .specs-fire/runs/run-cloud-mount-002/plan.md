# Plan: wizard-add-mount-mode-ui

**Run:** run-cloud-mount-002
**Work Item:** Replace "Get started" with "Close" in add-mount wizard mode
**Mode:** autopilot
**Created:** 2026-03-08

---

## Approach

Add a module-level `addMountMode` flag to `wizard.js`. When `goToAddMount()` is called (by tray.rs via `win.eval("goToAddMount()")`), it sets the flag before entering the sources step. The flag gates three behaviors:

1. **`loadSources()`** — after loading, when in add-mount mode, hides the OneDrive section (already mounted) and reconfigures the action button to "Close" (enabled).
2. **`updateGetStartedBtn()`** — returns early (no-op) when in add-mount mode, preventing the OneDrive checkbox state from disabling/enabling the button.
3. **`get-started-btn` click handler** — in add-mount mode, closes the Tauri window; otherwise calls `getStarted()`.

No HTML structural changes needed. Button label/state is fully controlled via JS.

---

## Files to Modify

| File | Change |
|------|--------|
| `crates/cloudmount-app/dist/wizard.js` | Add flag, guard functions, update button handler |

## Files to Create

(none)

## Tests

`wizard.js` is a dist frontend file — no automated JS test framework exists. Validation is via:
- Manual logic review: all acceptance criteria traced through code
- Rust test suite (`cargo test -p cloudmount-app`) must still pass (no Rust changes)

## Acceptance Criteria Checklist

- [ ] `addMountMode` flag (default `false`) at module level
- [ ] `goToAddMount()` sets `addMountMode = true` before `onSignInComplete()`
- [ ] `loadSources()` hides `#sources-onedrive-section` and sets button to "Close"/enabled when `addMountMode`
- [ ] `updateGetStartedBtn()` is no-op when `addMountMode`
- [ ] `get-started-btn` click closes window when `addMountMode`, calls `getStarted()` otherwise
- [ ] Initial setup path (sign in → sources → "Get started") unaffected
- [ ] No new Tauri commands added
