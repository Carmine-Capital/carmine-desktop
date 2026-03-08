---
id: settings-dark-redesign
title: Apply dark premium design to settings
intent: ui-dark-premium-redesign
complexity: medium
mode: confirm
status: completed
depends_on:
  - design-system-setup
created: 2026-03-08T12:00:00Z
run_id: run-cloud-mount-003
completed_at: 2026-03-08T12:32:30.662Z
---

# Work Item: Apply dark premium design to settings

## Description

Redesign `settings.html` and update `settings.js` to apply the dark premium Violet/Space design
using the shared `styles.css` and `ui.js`. The inline `<style>` block in `settings.html` is
removed. All four panels — General, Mounts, Account, Advanced — get a full visual pass. The
`settings.js` status bar implementation is replaced with a call to `showStatus()` from `ui.js`.

## Acceptance Criteria

- [ ] Inline `<style>` block in `settings.html` is removed (all styles come from `styles.css`)
- [ ] `body` renders with `--bg-base` background, Inter font, `--text-primary` color
- [ ] **Tab bar**: `--bg-surface` background, `--border` bottom border, active tab shows `--accent` underline and `--text-primary` color, inactive tabs show `--text-secondary`
- [ ] **General panel**: checkbox fields styled cleanly; `<select>` uses `.input` dark style; Save button uses `.btn`
- [ ] **Mounts panel**: each mount item rendered as a `.card` with mount name in `--text-primary` and path in `--text-secondary`; Add Mount button uses `.btn`; per-mount remove/toggle buttons styled appropriately
- [ ] **Account panel**: account email displayed in `--text-primary`; Sign Out button uses `.btn-danger`
- [ ] **Advanced panel**: all `<input>` and `<select>` use `.input`; Save uses `.btn`; Clear Cache uses `.btn-danger`
- [ ] **Status bar**: uses `showStatus()` from `ui.js` — the inline status bar CSS and JS in `settings.js` are removed; `#status-bar` element in HTML is removed (or kept as the mount point for `showStatus`)
- [ ] No hardcoded hex colors remain in `settings.html` inline styles
- [ ] All form labels use `--text-secondary` at `0.875rem`
- [ ] Visual result matches the wizard's design language — same font, same color palette, same component feel

## Technical Notes

- `settings.js` currently has its own `showStatus` implementation — remove it and call `ui.js`'s version instead
- The `showStatus` in `settings.js` targets `#status-bar` by ID — `ui.js` version should do the same (consistent target)
- Tab switching is done by `data-panel` attribute — keep JS logic, only change visual classes
- The `.danger` class on buttons should become `.btn-danger` to match the design system
- Checkboxes: use standard `<input type="checkbox">` styling or a custom toggle — keep it simple and consistent with the dark theme
- The `<select>` elements need `appearance: none` and a custom dark background to override OS defaults

## Dependencies

- design-system-setup
