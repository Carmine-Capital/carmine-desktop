---
id: wizard-add-mount-mode-ui
title: Replace "Get started" with "Close" in add-mount wizard mode
intent: fix-wizard-add-mount-ux
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-002
completed_at: 2026-03-08T11:35:09.615Z
---

# Work Item: Replace "Get started" with "Close" in add-mount wizard mode

## Description

Modify `wizard.js` and `wizard.html` to make the wizard aware of its mode (initial setup vs. add-mount). In add-mount mode:
- Set `addMountMode = true` inside `goToAddMount()` (called by tray.rs via `win.eval`)
- After `loadSources()`, hide the OneDrive section (it is already mounted)
- Replace "Get started" button behavior: change label to "Close", always enabled, click closes the window
- Guard `updateGetStartedBtn()` so it does nothing in add-mount mode

Initial setup flow must be completely unaffected.

## Acceptance Criteria

- [ ] A module-level `addMountMode` flag (default `false`) exists in `wizard.js`
- [ ] `goToAddMount()` sets `addMountMode = true` before calling `onSignInComplete()`
- [ ] In `loadSources()`, when `addMountMode` is true, the OneDrive section (`#sources-onedrive-section`) is hidden and the button label is set to "Close" and enabled
- [ ] `updateGetStartedBtn()` is a no-op when `addMountMode` is true
- [ ] The click handler for the action button (currently "get-started-btn") closes the window when `addMountMode` is true, and calls `getStarted()` otherwise
- [ ] Clicking "Close" in add-mount mode closes the wizard window via Tauri API
- [ ] Initial setup path (sign in → step-sources → "Get started") behavior is unchanged
- [ ] No new Tauri commands added

## Technical Notes

Key files:
- `crates/cloudmount-app/dist/wizard.js` — add flag, guard functions, update button
- `crates/cloudmount-app/dist/wizard.html` — no structural changes needed; button label/state controlled via JS

The action button (`#get-started-btn`) can stay as-is in the HTML. JS handles all mode logic:

```js
let addMountMode = false;

async function goToAddMount() {
  addMountMode = true;
  await onSignInComplete();
}

// In loadSources(), after loading, add:
if (addMountMode) {
  document.getElementById('sources-onedrive-section').style.display = 'none';
  const btn = document.getElementById('get-started-btn');
  btn.textContent = 'Close';
  btn.disabled = false;
}

// updateGetStartedBtn() guard:
function updateGetStartedBtn() {
  if (addMountMode) return;
  // existing logic...
}

// In init(), update click handler:
document.getElementById('get-started-btn').addEventListener('click', () => {
  if (addMountMode) {
    window.__TAURI__.window.getCurrentWindow().close();
  } else {
    getStarted();
  }
});
```

## Dependencies

(none)
