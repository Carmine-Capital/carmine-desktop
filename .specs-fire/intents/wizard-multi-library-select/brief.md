---
id: wizard-multi-library-select
title: Wizard multi-library selection UX
status: completed
created: 2026-03-09T10:00:00Z
completed_at: 2026-03-09T16:25:13.599Z
---

# Intent: Wizard multi-library selection UX

## Goal

Redesign the SharePoint library selection step in the wizard so users can select multiple libraries from a site in one go, confirm them all at once, and never see already-mounted libraries as available options.

## Users

All CloudMount users who mount SharePoint document libraries — especially users managing multiple libraries across sites.

## Problem

Today the wizard forces a one-library-at-a-time flow: after each library pick the view resets to the site list, requiring the user to re-navigate to the same site. Already-mounted libraries still appear as selectable options, creating confusion. The overall flow feels tedious and error-prone for non-technical users.

## Success Criteria

- User can check/select multiple libraries from a single site before confirming
- A single "Add selected" action creates mounts for all checked libraries at once
- Already-mounted libraries are visually distinguished (greyed out with a check mark) and non-selectable
- The wizard stays on the library list after selection — no forced reset to the site list
- The flow feels smooth and obvious for a non-technical user
- Consistent with the existing dark premium design system

## Constraints

- Must work within the existing Tauri command architecture (add_mount, list_drives, list_mounts)
- One MountConfig per library in the config model (unchanged)
- Vanilla JS, no framework — event handlers via addEventListener (CSP constraint)
- Must handle edge cases: duplicate mount points, library already mounted by another account

## Notes

Key files: wizard.html, wizard.js, commands.rs. The current selectSiteInSources() and mountLibraryInSources() functions in wizard.js are the main targets. Backend add_mount is called per library (loop on frontend side).
