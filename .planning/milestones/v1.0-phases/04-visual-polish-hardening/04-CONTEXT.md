# Phase 4: Visual Polish & Hardening - Context

**Gathered:** 2026-03-19
**Status:** Ready for planning

<domain>
## Phase Boundary

Modernize the UI visual design and ensure every user-initiated action provides visible feedback. The app must look professional and feel cohesive — ready for org-wide Windows deployment. No new features, no new panels, no new data sources. This is a refinement pass on existing UI surfaces (settings.html, wizard.html) using the existing vanilla JS + CSS architecture.

Requirements covered: UI-01, UI-02.

</domain>

<decisions>
## Implementation Decisions

### Color palette direction
- **Soft dark refresh** — lighten the dark palette, not switch to light mode. Less cave-like, more refined.
- Lift background tokens: `--bg-base` from near-black to a slightly lighter dark gray, `--bg-surface` and `--bg-elevated` follow proportionally. Target: still unmistakably dark mode but with more visual lift and less heaviness.
- Lift `--text-muted` for better readability — currently `#5c6080`, needs to be more visible.
- Keep `--text-primary` (#e8e9f0) and `--text-secondary` (#8c90aa) as-is or with minor tweaks.
- Keep `color-scheme: dark` — this is a palette refinement, not a mode switch.

### Accent color
- **Keep carmine red (#99222E) unchanged** — brand identity. Adjust hover/active variants only if needed for contrast against new backgrounds.

### Typography scale
- Adjust font sizes and weights for better readability and hierarchy.
- Current body text is 12.5-13px — may need slight increase for comfort.
- Tighten heading hierarchy — section headings, card titles, labels should have clear visual distinction.
- Review font-weight usage — ensure a clear system (400 body, 500 labels, 600 headings).

### Surface depth & borders
- Rework card/panel borders and shadows — less hard borders, more subtle elevation differences.
- Softer visual layers between base, surface, and elevated backgrounds.
- Reduce reliance on hard `1px solid` borders where background contrast alone can create separation.
- Refine `--border` and `--border-row` tokens — potentially softer or warmer alpha values.
- Subtle shadow refinements on elevated elements (drive cards, error entries).

### Component polish
- Refine buttons, inputs, toggles, and badges — rounder corners, better hover states, smoother transitions.
- More tactile feel on interactive elements.
- Status dots, health badges, activity tags — ensure consistent sizing and alignment.
- Toggle switches — review track/thumb sizing and animation smoothness.
- Input fields — consistent styling across text, number, and select elements.

### Whitespace & density
- Adjust padding and margins globally for better breathing room between sections.
- Review spacing between dashboard sections (drives, activity, errors, cache).
- Ensure consistent vertical rhythm throughout all panels.
- Sidebar padding and nav item spacing review.

### Claude's Discretion
- Exact color values for the refreshed palette (within the soft dark direction)
- Specific border-radius values for rounder corners
- Shadow values and elevation scale
- Transition timing and easing curves
- Whether to adjust the wizard page proportionally or leave it for a separate pass
- How to handle the status bar styling in the new palette
- Whether `--accent-bg` opacity needs adjustment against lighter backgrounds
- Action feedback audit — identifying which user actions lack `showStatus()` calls and adding them (UI-02)
- Cross-platform font rendering adjustments if needed

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Current UI implementation (modify these)
- `crates/carminedesktop-app/dist/styles.css` — Full design token system and all component styles. Primary target for visual changes.
- `crates/carminedesktop-app/dist/settings.html` — Dashboard + settings panels HTML structure. May need minor structural changes for spacing.
- `crates/carminedesktop-app/dist/settings.js` — JS rendering functions that produce DOM elements — some have inline styles that need migration to CSS classes.
- `crates/carminedesktop-app/dist/ui.js` — `showStatus()` feedback function. Review for UI-02 completeness.
- `crates/carminedesktop-app/dist/wizard.html` — Setup wizard HTML. Should receive proportional visual updates.
- `crates/carminedesktop-app/dist/wizard.js` — Wizard JS. Review for action feedback gaps (UI-02).

### Requirements
- `.planning/REQUIREMENTS.md` — UI-01 (visual modernization) and UI-02 (action feedback) acceptance criteria.

### Phase 3 context (design decisions to preserve)
- `.planning/phases/03-dashboard-ui/03-CONTEXT.md` — Dashboard layout decisions, section ordering, component patterns. Visual polish must preserve these structural decisions.

### CSP constraint
- `crates/carminedesktop-app/tauri.conf.json` — CSP policy: `script-src 'self'`. No inline event handlers.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Design token system in `:root`** — Full set of tokens (colors, spacing, radii, shadows). Phase 4 modifies token values, not the token architecture.
- **Button variants** — `.btn-ghost`, `.btn-danger`, `.btn-link`, `.btn-sm`, `.btn-icon` — refine these, don't restructure.
- **`showStatus()` in `ui.js`** — Existing feedback mechanism with success/error/info modes. Audit usage for UI-02.
- **Event delegation pattern** — `data-action` attributes on dynamic elements. No changes needed.
- **`scheduleRender()` with `requestAnimationFrame`** — Debounced rendering. No changes needed.

### Established Patterns
- **Token-based styling** — All colors, spacing, radii reference CSS custom properties. Changes to `:root` values cascade automatically.
- **Component class naming** — `.drive-card`, `.activity-row`, `.error-entry`, `.health-badge` — consistent BEM-ish naming.
- **`@media (prefers-reduced-motion: reduce)`** — Already handles reduced motion. Preserve this.
- **Inline styles in JS** — Some render functions use `element.style.x = ...` instead of CSS classes (e.g., `renderHandlers` sets `display: flex` inline). These should be migrated to proper CSS classes during polish.

### Integration Points
- **`:root` token block (styles.css:20-50)** — Primary modification target. All component styles reference these tokens.
- **Block 3-4 (styles.css:62-173)** — Button and input styles. Refine border-radius, padding, transitions.
- **Block 5 (styles.css:176-237)** — Sidebar layout. Review padding and nav item spacing.
- **Block 6 (styles.css:239-261)** — Section headings and setting rows. Review spacing and typography.
- **Dashboard styles (styles.css:298-564)** — Card, activity, error, cache styles. Refine borders, shadows, spacing.

</code_context>

<specifics>
## Specific Ideas

- "More refined, modern and less dark" — the overall direction. Think Linear/Raycast soft dark, not stark terminal-black.
- The carmine red accent is brand identity — don't dilute it.
- All four refinement areas selected with equal priority: typography, surfaces, components, whitespace.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 04-visual-polish-hardening*
*Context gathered: 2026-03-19*
