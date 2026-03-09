# Implementation Plan — fix-frontend-accessibility

**Run**: run-cloud-mount-018
**Work Item**: fix-frontend-accessibility
**Intent**: fix-comprehensive-review
**Mode**: confirm

## Approach

CSS-only and HTML-attribute changes across `styles.css`, `wizard.html`, and `settings.html`. No JS changes needed. Each fix is isolated, low-risk, and addresses a specific WCAG or UX issue.

## Files to Modify

### 1. `crates/cloudmount-app/dist/styles.css`

| # | Fix | Location | Change |
|---|-----|----------|--------|
| 1 | `color-scheme: dark` | `:root` block (~line 10) | Add `color-scheme: dark;` so browser renders scrollbars/form controls in dark mode |
| 2 | Button focus-visible | After button rules (~line 85) | Add `button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }` |
| 3 | Input/select focus-visible | Replace `outline: none` (~line 137) | Change `outline: none` to `outline: 2px solid transparent; outline-offset: 2px;`, add `:focus-visible` rule with `outline-color: var(--accent)` |
| 4 | Select dropdown arrow | After input/select block (~line 141) | Add `select { background-image: url(...SVG...); background-repeat: no-repeat; background-position: right 0.75rem center; padding-right: 2rem; }` |
| 5 | Section heading contrast | `.section-heading` (~line 185) | Change `color: var(--text-muted)` to `color: var(--text-secondary)` (3.5:1 → 5.8:1) |
| 6 | Status dismiss button min-size | `.status-dismiss` (~line 209) | Add `min-width: 2rem; min-height: 2rem;` for 32px touch target |
| 7 | `prefers-reduced-motion` | After spinner rules (~line 168) | Add `@media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }` |
| 8 | Overflow/ellipsis | 6 selectors | Add `overflow: hidden; text-overflow: ellipsis; white-space: nowrap;` to `.mount-path`, `.sp-result-url`, `.url-input`, `.added-source-name`, `.source-card-sub` |
| 9 | `.sp-result-row` focus | ~line 386-390 | Replace `outline: none` with `outline-color: transparent` for forced-colors compatibility |
| 10 | `.sp-lib-row` focus | ~line 469-473 | Replace `outline: none` with `outline-color: transparent` for forced-colors compatibility |

### 2. `crates/cloudmount-app/dist/wizard.html`

| # | Fix | Location | Change |
|---|-----|----------|--------|
| 1 | aria-label on auth URL input | Line 25 | Add `aria-label="Authentication URL"` |
| 2 | aria-label on SP search input | Line 48 | Add `aria-label="Search SharePoint sites"` |

### 3. `crates/cloudmount-app/dist/settings.html`

No changes needed — tabs already have ARIA attributes, inputs have associated labels.

## Files to Create

None.

## Tests

No automated tests — these are CSS/HTML accessibility fixes. Validation is manual:
- Keyboard-tab through wizard and settings, verify visible focus rings on all interactive elements
- Verify select dropdowns show custom arrow
- Verify long text truncates with ellipsis
- Verify section headings are readable (contrast)
- Verify spinner stops in prefers-reduced-motion
- Use forced-colors mode to verify focus indicators

## Risk Assessment

**Low risk** — All changes are additive CSS rules or HTML attributes. No JS logic changes. No existing behavior modified beyond visual presentation.
