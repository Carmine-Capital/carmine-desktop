# Test Report: wizard-add-mount-mode-ui

**Run:** run-cloud-mount-002
**Work Item:** Replace "Get started" with "Close" in add-mount wizard mode
**Date:** 2026-03-08

---

## Test Results

| Suite | Passed | Failed | Skipped |
|-------|--------|--------|---------|
| cloudmount-app unit tests | 6 | 0 | 0 |
| cloudmount-app integration tests | 12 | 0 | 2 (live API) |
| **Total** | **18** | **0** | **2** |

```
test result: ok. 6 passed; 0 failed; 0 ignored (unit)
test result: ok. 12 passed; 0 failed; 2 ignored (integration — requires live Graph API)
```

## Coverage Notes

- `wizard.js` is a browser-side dist file — no automated JS test framework exists in this project
- Rust test suite validates no regressions in backend behaviour (no Rust files changed)
- Frontend logic validated via static analysis against all acceptance criteria below

## Acceptance Criteria Validation

| Criterion | Status | Evidence |
|-----------|--------|---------|
| `addMountMode` flag (default `false`) at module level | ✅ PASS | `wizard.js:9` — `let addMountMode = false;` |
| `goToAddMount()` sets `addMountMode = true` before `onSignInComplete()` | ✅ PASS | `wizard.js:75-78` |
| `loadSources()` hides `#sources-onedrive-section` when `addMountMode` | ✅ PASS | `wizard.js:107-112` — applied after drive block |
| `loadSources()` sets button label to "Close" and enables it when `addMountMode` | ✅ PASS | `wizard.js:109-111` |
| `updateGetStartedBtn()` is no-op when `addMountMode` | ✅ PASS | `wizard.js:291` — early return |
| `get-started-btn` click closes window when `addMountMode` | ✅ PASS | `wizard.js:354-360` |
| `get-started-btn` click calls `getStarted()` when NOT `addMountMode` | ✅ PASS | `wizard.js:357-359` |
| Initial setup path (sign in → sources → "Get started") unaffected | ✅ PASS | `addMountMode` stays `false`; `updateGetStartedBtn()` and `getStarted()` paths unchanged |
| No new Tauri commands added | ✅ PASS | Only `wizard.js` modified; no `commands.rs` changes |

## Edge Cases Verified

- **Search input cleared in add-mount mode**: `onSourcesSpSearchInput()` re-calls `loadSources()` → add-mount mode block re-applies correctly (OneDrive stays hidden, button stays "Close"/enabled)
- **Drive fetch fails in add-mount mode**: `driveResult` rejected → OneDrive section was never shown → add-mount mode block is still reached and sets button correctly (section was already hidden)
- **Sites fetch fails in add-mount mode**: SP section stays hidden; add-mount mode block still runs independently
