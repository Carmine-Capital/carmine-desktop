---
id: design-system-setup
title: Create shared design system (tokens, Inter font, shared CSS/JS)
intent: ui-dark-premium-redesign
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T12:00:00Z
run_id: run-cloud-mount-003
completed_at: 2026-03-08T12:32:20.223Z
---

# Work Item: Create shared design system (tokens, Inter font, shared CSS/JS)

## Description

Create the shared foundation that all UI surfaces will build on. This includes bundling the Inter
font locally, a single `dist/styles.css` with all design tokens as CSS custom properties and
reusable component styles, and a `dist/ui.js` with shared JS utilities. Both `wizard.html` and
`settings.html` get `<link>` and `<script>` tags pointing to these shared files — but the pages
are not yet visually restyled in this work item.

## Acceptance Criteria

- [ ] `dist/fonts/` contains Inter woff2 files (variable font or 400/500/600 subsets) — sourced from the official Inter release, no CDN
- [ ] `dist/styles.css` defines the full Violet/Space token table as CSS custom properties on `:root`
- [ ] `dist/styles.css` includes a minimal CSS reset (`box-sizing`, margin/padding zero)
- [ ] `dist/styles.css` includes a `@font-face` declaration referencing the local Inter files
- [ ] `dist/styles.css` includes reusable component classes: `.btn` (primary), `.btn-secondary`, `.btn-danger`, `.btn-sm`, `.input`, `.card`, `.spinner`, `.badge`, `.tabs`/`.tab`, `.section-heading`, `.status-bar`
- [ ] `dist/ui.js` exports a `showStatus(message, type)` function (type: `'success'|'error'|'info'`) for toast/status-bar notifications
- [ ] `wizard.html` has `<link rel="stylesheet" href="styles.css">` added (before existing `<style>`)
- [ ] `settings.html` has `<link rel="stylesheet" href="styles.css">` added
- [ ] `wizard.js` imports or references `ui.js` (or `ui.js` is loaded via `<script src="ui.js">` before `wizard.js`)
- [ ] `settings.js` uses `showStatus()` from `ui.js` instead of its own inline implementation
- [ ] Build passes (`cargo build -p cloudmount-app`)

## Technical Notes

### Token Table

```css
:root {
  /* Backgrounds */
  --bg-base:       #0e0f14;
  --bg-surface:    #16181f;
  --bg-elevated:   #1e2028;
  --border:        #2a2d3a;

  /* Accent */
  --accent:        #7c5cfc;
  --accent-hover:  #9074ff;

  /* Text */
  --text-primary:  #edeef2;
  --text-secondary:#8b8fa8;
  --text-muted:    #5c607a;

  /* Semantic */
  --success:       #22c55e;
  --danger:        #f04747;

  /* Spacing scale */
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-6: 1.5rem;
  --space-8: 2rem;

  /* Radius */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;

  /* Shadows */
  --shadow-sm: 0 1px 3px rgba(0,0,0,0.4);
  --shadow-md: 0 4px 16px rgba(0,0,0,0.5);
  --shadow-glow: 0 0 20px rgba(124,92,252,0.2);
}
```

### Inter Font

Download from https://github.com/rsms/inter/releases — use the variable font
`InterVariable.woff2` for a single file, or the static 400/500/600 subset woff2s.
Place in `dist/fonts/`.

### ui.js showStatus

`showStatus(message, type)` should:
- Find or create a `#status-bar` element
- Set text content and a class for the type
- Auto-dismiss after ~3s
- This is already implemented in `settings.js` — extract it into `ui.js`

## Dependencies

(none)
