---
id: fix-wizard-add-mount-ux
title: Fix wizard add-mount mode UX
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T11:35:09.620Z
---

# Intent: Fix wizard add-mount mode UX

## Goal

When the user opens the wizard via tray → "Add Mount…", the wizard should show a "Close" button (not "Get started"), hide the OneDrive section (already mounted), and never attempt to re-add an already-mounted drive.

## Users

CloudMount desktop users who want to add additional SharePoint libraries after initial sign-in and setup.

## Problem

The wizard has two entry paths:
1. **Initial setup** (not yet authenticated): sign-in → sources → "Get started" adds OneDrive + completes wizard
2. **Add mount** (already authenticated): goes directly to sources step via `goToAddMount()`

In add-mount mode, the wizard reuses the same step-sources UI without adapting it:
- OneDrive checkbox is shown pre-checked → `updateGetStartedBtn()` immediately enables "Get started"
- Clicking "Get started" calls `getStarted()`, which tries to `add_mount` OneDrive → error: "mount point already in use"
- "Get started" is semantically wrong: SharePoint mounts are applied immediately when selected (via `mountLibraryInSources()`), not deferred to the button

## Success Criteria

- In add-mount mode, "Get started" button is replaced by an always-enabled "Close" button
- In add-mount mode, the OneDrive section is hidden (it is already mounted)
- Clicking "Close" closes the wizard window
- SharePoint library selection still works and mounts immediately on click
- Initial setup flow (first-time sign-in path) is completely unaffected

## Constraints

- Frontend only: `crates/cloudmount-app/dist/wizard.js` and `wizard.html`
- No new Tauri commands needed
- Must not break the existing `goToAddMount()` contract used by `tray.rs`

## Notes

`tray.rs::open_or_focus_wizard(app, true)` calls `win.eval("goToAddMount()")` to enter add-mount mode — so the mode flag must be set inside `goToAddMount()`.
