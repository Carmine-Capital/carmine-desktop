---
id: fix-frontend-accessibility
title: Focus indicators, select arrow, aria-labels, contrast, overflow
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-018
completed_at: 2026-03-09T19:26:47.364Z
---

# Work Item: Focus indicators, select arrow, aria-labels, contrast, overflow

## Description

Fix accessibility and CSS issues across the frontend:

1. **No focus-visible on buttons** (`styles.css:70`): Add `button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }`.

2. **outline:none on inputs** (`styles.css:137`): Replace with `:focus-visible` rule showing accent outline. Keep border-color change as complement.

3. **Select no dropdown arrow** (`styles.css:139`): `appearance: none` removes native arrow. Add custom SVG arrow via `background-image` with `padding-right: 2rem`.

4. **Missing aria-labels** (`wizard.html:25,48`): Add `aria-label="Authentication URL"` and `aria-label="Search SharePoint sites"`.

5. **Section heading contrast** (`styles.css:185`): `var(--text-muted)` (#5c607a) on dark bg is 3.5:1. Change to `var(--text-secondary)` (#8b8fa8) for 5.8:1.

6. **Missing overflow/ellipsis** (6 locations): Add `overflow: hidden; text-overflow: ellipsis; white-space: nowrap;` to `.mount-path`, `.sp-result-url`, `.url-input`, `.added-source-name`, `.source-card-sub`.

7. **color-scheme: dark** (`styles.css:10`): Add to `:root` so browser renders scrollbars/form controls in dark mode.

8. **prefers-reduced-motion** (`styles.css:67`): Add `@media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }`.

9. **Dismiss button too small** (`styles.css:209`): Add `min-width: 2rem; min-height: 2rem;` for 32px target.

10. **focus-visible on .sp-result-row/.sp-lib-row** (`styles.css:386,469`): Replace `outline: none` with `outline-color: transparent` so forced-colors mode works.

## Acceptance Criteria

- [ ] All buttons have visible focus ring on keyboard navigation
- [ ] All inputs have visible focus ring (not just border-color change)
- [ ] Select elements show a custom dropdown arrow
- [ ] Search and auth URL inputs have aria-label attributes
- [ ] Section headings meet WCAG AA contrast (4.5:1+)
- [ ] Long paths/URLs truncate with ellipsis instead of overflowing
- [ ] color-scheme: dark declared on :root
- [ ] Spinner respects prefers-reduced-motion
- [ ] Dismiss button has minimum 32px click target
- [ ] Focus indicators work in forced-colors/high-contrast mode

## Technical Notes

For the select arrow, use an inline SVG data URI: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%238b8fa8' d='M2 4l4 4 4-4'/%3E%3C/svg%3E")`.

## Dependencies

(none)
