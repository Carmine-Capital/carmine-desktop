---
id: fix-wizard-ux
title: Sanitize paths, back navigation, FUSE pre-check, auth timeout UX
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-020
completed_at: 2026-03-09T19:32:14.891Z
---

# Work Item: Sanitize paths, back navigation, FUSE pre-check, auth timeout UX

## Description

Fix wizard UX issues:

1. **Unsanitized display_name** (`wizard.js:336`): Mount path `'~/Cloud/' + site.display_name + ' - ' + library.name` uses raw API values that may contain `/`, `\`, or other filesystem-unsafe chars. Fix: strip or replace `[/\\:*?"<>|]` with `_` before constructing path.

2. **No back navigation** (`wizard.html`): Once on step-sources, no way to go back or switch accounts. Fix: add "Sign in with a different account" link that calls sign_out then returns to step-welcome.

3. **FUSE pre-wizard check** (`main.rs:506`): FUSE unavailability notification sent post-auth. Fix: check FUSE availability in `preflight_checks()` and block wizard with clear message if FUSE is missing, before the user goes through auth.

4. **~/Cloud/ on Windows** (`wizard.js:336,458`): `~/Cloud/` is Unix convention. Fix: call a Tauri command (e.g., `get_default_mount_root()`) that returns the platform-appropriate expanded path, or use `expand_mount_point()` on the Rust side and return the result.

5. **Auth timeout no countdown** (`oauth.rs:86`): 120s timeout with no UI feedback. Fix: show a countdown timer in the wizard signing-in step. Use `setInterval` in JS to update a "Time remaining: Xs" display. Show warning when <30s remain.

## Acceptance Criteria

- [ ] Mount path display_name and library_name are sanitized (no filesystem-unsafe chars)
- [ ] Sources step has a "Sign in with different account" option
- [ ] FUSE check blocks wizard entry on Linux if FUSE is unavailable
- [ ] Mount paths show platform-native format (not ~/Cloud/ on Windows)
- [ ] Auth flow shows countdown timer with warning near expiry
- [ ] All changes work within CSP (addEventListener, no inline handlers)

## Technical Notes

For FUSE pre-check, add a Tauri command `check_fuse_available()` that returns bool. Wizard calls it before starting auth; if false, show error step instead of auth step.

For the mount root, add `get_default_mount_root` command that calls `expand_mount_point("~/Cloud/")` and returns the expanded string.

## Dependencies

(none)
