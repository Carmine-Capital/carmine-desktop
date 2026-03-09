---
id: ux-polish
title: Minor UX polish (dismiss, empty states, caching, title)
intent: fix-review-findings
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T17:00:00Z
run_id: run-cloud-mount-011
completed_at: 2026-03-09T17:36:41.542Z
---

# Work Item: Minor UX polish (dismiss, empty states, caching, title)

## Description

Fix 5 minor UX issues:

- **D3**: Error status bar has no dismiss mechanism — add a close button/icon to the status bar that hides it on click
- **M4**: Clearing search in wizard triggers full `loadSources()` re-fetch — cache the initial followed-sites result and restore from cache when search is cleared
- **M5**: Selecting a new site silently clears library selections from previous site — either preserve cross-site selections or warn the user
- **M6**: Empty mount list in settings shows blank panel — add "No mounts configured" empty state message
- **M7**: Document title stays "CloudMount Setup" across all wizard steps — update `<title>` to reflect current step

## Acceptance Criteria

- [ ] Error status bar shows a dismiss/close button; clicking it hides the bar
- [ ] Clearing search field in wizard restores cached sites without network re-fetch
- [ ] Switching sites in wizard either preserves prior selections or shows a brief warning
- [ ] Settings mount list shows "No mounts configured" when empty
- [ ] Wizard document title updates per step (e.g., "Sign In - CloudMount Setup")
- [ ] No inline event handlers (CSP compliant)

## Technical Notes

Changes span `wizard.js`, `settings.js`, `wizard.html`, `settings.html`, and `styles.css` (close button styling). For M4, store the result of `get_followed_sites` in a module-level variable and filter locally. For M5, simplest approach is to warn — preserving cross-site selections adds significant complexity.

## Dependencies

(none)
