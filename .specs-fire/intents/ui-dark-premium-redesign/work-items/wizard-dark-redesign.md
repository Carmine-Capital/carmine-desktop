---
id: wizard-dark-redesign
title: Apply dark premium design to wizard
intent: ui-dark-premium-redesign
complexity: medium
mode: confirm
status: completed
depends_on:
  - design-system-setup
created: 2026-03-08T12:00:00Z
run_id: run-cloud-mount-003
completed_at: 2026-03-08T12:32:23.467Z
---

# Work Item: Apply dark premium design to wizard

## Description

Redesign `wizard.html` and update `wizard.js` to apply the dark premium Violet/Space design
using the shared `styles.css` and `ui.js` established in `design-system-setup`. All four wizard
steps — welcome, signing-in, sources, and success — get a full visual pass. The inline `<style>`
block in `wizard.html` is removed and replaced with design-system classes and CSS custom
properties. This work item also absorbs the "Get started" → "Close" UX fix for add-mount mode.

## Acceptance Criteria

- [ ] Inline `<style>` block in `wizard.html` is removed (all styles come from `styles.css`)
- [ ] `body` renders with `--bg-base` background, Inter font, `--text-primary` color
- [ ] **Welcome step**: centered layout with CloudMount name in `--text-primary`, subtitle in `--text-secondary`, sign-in button uses `.btn` (accent violet), subtle vertical spacing with generous padding
- [ ] **Signing-in step**: dark spinner using `--accent` border-top-color, URL input uses `.input` (dark, `--bg-elevated` bg), Copy and Cancel buttons styled correctly
- [ ] **Sources step**: source cards use `.card` with `--bg-surface` bg, hover shows `--accent` border glow; section headings use `--text-muted` uppercase; SharePoint search input uses `.input`; SP result rows are dark cards with hover state; "Added" items row shows `.btn-danger` remove button
- [ ] **Success step**: mount list items styled as small `.card` entries, close button uses `.btn`
- [ ] **Add-mount mode**: the "Get started" button is replaced with "Close" (button ID: `get-started-btn` updated or replaced — must match `wizard.js` logic)
- [ ] Error messages use `--danger` color (not `#c00`)
- [ ] Spinner animation uses CSS custom property colors
- [ ] No hardcoded hex colors remain in `wizard.html` inline styles
- [ ] Visual result: dark backgrounds, violet accent CTAs, Inter font, consistent spacing

## Technical Notes

- The `wizard.js` file should not need structural changes — only cosmetic ones (error message styling, any inline `style` mutations that set colors should use CSS variables instead)
- The URL box input is `readonly` — style it distinctly (monospace, `--bg-elevated`, muted text)
- For the welcome step, consider a subtle violet radial glow behind the CloudMount title (CSS only, `radial-gradient` on a `::before` pseudo-element)
- Source cards with checkboxes: the `<label>` wrapping trick must remain — only visual changes
- The spinner border colors: `border-color: var(--border)` for the base ring, `border-top-color: var(--accent)` for the active arc
- SP back-link becomes a text button styled with `--accent` color and no background

## Dependencies

- design-system-setup
