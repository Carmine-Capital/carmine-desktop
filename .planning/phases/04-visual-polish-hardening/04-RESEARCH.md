# Phase 4: Visual Polish & Hardening - Research

**Researched:** 2026-03-19
**Domain:** CSS design token refinement, vanilla JS feedback audit, cross-platform Tauri UI
**Confidence:** HIGH

## Summary

Phase 4 is a visual refinement pass on two existing UI surfaces (`settings.html`, `wizard.html`) using the project's established vanilla CSS custom property token system. No new dependencies, no new panels, no structural changes. The UI-SPEC (`04-UI-SPEC.md`) provides a complete design contract with exact token values, component specifications, typography consolidation, and spacing scale normalization.

The scope decomposes into three workstreams: (1) design token and CSS updates in `styles.css`, (2) inline style migration from JS/HTML to CSS classes, and (3) UI-02 action feedback fixes in `wizard.js`. All changes are frontend-only -- no Rust code changes required. The token architecture is already well-designed; Phase 4 modifies token *values* and refines component *rules*, not the architecture itself.

**Primary recommendation:** Execute as two plans -- first the CSS token/component overhaul (bulk of work), then the JS inline-style migration plus UI-02 feedback fixes. The token changes cascade automatically, so most visual updates happen by editing `:root` values alone.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Soft dark refresh** -- lighten the dark palette, not switch to light mode. Less cave-like, more refined.
- Lift background tokens: `--bg-base` from near-black to a slightly lighter dark gray, `--bg-surface` and `--bg-elevated` follow proportionally. Target: still unmistakably dark mode but with more visual lift and less heaviness.
- Lift `--text-muted` for better readability -- currently `#5c6080`, needs to be more visible.
- Keep `--text-primary` (#e8e9f0) and `--text-secondary` (#8c90aa) as-is or with minor tweaks.
- Keep `color-scheme: dark` -- this is a palette refinement, not a mode switch.
- **Keep carmine red (#99222E) unchanged** -- brand identity. Adjust hover/active variants only if needed for contrast against new backgrounds.
- Adjust font sizes and weights for better readability and hierarchy.
- Rework card/panel borders and shadows -- less hard borders, more subtle elevation differences.
- Refine buttons, inputs, toggles, and badges -- rounder corners, better hover states, smoother transitions.
- Adjust padding and margins globally for better breathing room between sections.
- All four refinement areas selected with equal priority: typography, surfaces, components, whitespace.
- "More refined, modern and less dark" -- the overall direction. Think Linear/Raycast soft dark, not stark terminal-black.

### Claude's Discretion
- Exact color values for the refreshed palette (within the soft dark direction)
- Specific border-radius values for rounder corners
- Shadow values and elevation scale
- Transition timing and easing curves
- Whether to adjust the wizard page proportionally or leave it for a separate pass
- How to handle the status bar styling in the new palette
- Whether `--accent-bg` opacity needs adjustment against lighter backgrounds
- Action feedback audit -- identifying which user actions lack `showStatus()` calls and adding them (UI-02)
- Cross-platform font rendering adjustments if needed

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UI-01 | UI visual design is modernized with consistent styling, proper spacing, and professional appearance | UI-SPEC provides complete token values, typography scale, spacing normalization, component specifications, and border radius updates. All changes cascade through `:root` custom properties. |
| UI-02 | All user-facing actions provide visible feedback via status indicators -- no operation completes silently | UI-SPEC action feedback audit identifies 2 gaps: wizard `copyAuthUrl()` initial button text inconsistency and `removeSource()` missing success feedback. Both are in `wizard.js`. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Vanilla CSS | N/A | All styling via custom properties in `styles.css` | Project convention -- zero dependencies, token-based design system already in place |
| Vanilla JS | N/A | DOM manipulation in `settings.js`, `wizard.js`, `ui.js` | Project convention -- no build step, Tauri IPC via `window.__TAURI__` |
| Inter Variable | Bundled | Typography (already at `dist/fonts/InterVariable.woff2`) | Already in use, weight range 100-900 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| Tauri webview (Chromium) | 2.x | Rendering engine | All CSS features target Chromium -- no cross-browser concerns |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Vanilla CSS tokens | CSS preprocessor (Sass) | Project has zero build step for frontend -- adding preprocessor would break convention |
| Hand-tuned values | Design system library | Overkill for 2 HTML pages with existing token system |

**Installation:**
```bash
# No installation needed -- all dependencies already in place
```

## Architecture Patterns

### File Modification Map
```
crates/carminedesktop-app/dist/
  styles.css       # PRIMARY: token values, component rules, new utility classes
  settings.html    # MINOR: no changes expected (inline styles are display:none toggles)
  settings.js      # MINOR: migrate 3 inline style patterns to CSS classes
  wizard.html      # MINOR: migrate 4 inline style patterns to CSS classes
  wizard.js        # MINOR: 2 UI-02 feedback fixes
  ui.js            # NO CHANGES expected
```

### Pattern 1: Token Cascade
**What:** CSS custom properties in `:root` cascade to all components automatically. Changing `--bg-elevated` in one place updates drive cards, inputs, error entries, toggle tracks -- everywhere the token is referenced.
**When to use:** All color, spacing, radius, and shadow changes.
**Example:**
```css
/* Changing the token value updates ALL consumers */
:root {
  --bg-elevated: #2a2d3e;  /* was #252733 */
}
/* No need to touch .drive-card, input, .error-entry, etc. */
```

### Pattern 2: Inline Style to CSS Class Migration
**What:** Replace `element.style.x = value` in JS with a CSS class that achieves the same layout.
**When to use:** When JS render functions set visual properties that should be governed by the token system.
**Example:**
```css
/* NEW CSS class */
.handler-info {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
```
```javascript
/* BEFORE */
info.style.display = 'flex';
info.style.alignItems = 'center';
info.style.gap = '10px';

/* AFTER */
info.className = 'handler-info';
```

### Pattern 3: Preserving Display Toggle Inline Styles
**What:** Inline `style="display:none"` on HTML elements that JS toggles via `.style.display = ''` or `.style.display = 'none'` are NOT migration targets.
**When to use:** Leave these alone. They are visibility toggles, not visual styling. Migrating them to classes would require adding/removing classes in every JS toggle site.
**Critical distinction:** The UI-SPEC migration table correctly excludes these. Only migrate inline styles that set *visual* properties (flex layout, colors, margins).

### Anti-Patterns to Avoid
- **Touching display toggle inline styles:** `style="display:none"` in settings.html and wizard.html are JS visibility toggles. Do not migrate these.
- **Changing token names:** The `:root` token names (`--bg-base`, `--space-4`, etc.) are referenced across all CSS rules. Change values, not names. Two tokens are being removed (`--space-3`, `--space-5`) -- all usage sites must be remapped before removal.
- **Adding new dependencies:** No CSS preprocessors, no PostCSS, no UI libraries. Project convention is zero frontend dependencies.
- **Ignoring `@media (prefers-reduced-motion: reduce)`:** Already present, must be preserved and extended to cover any new transitions.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Design token system | Custom token architecture | Existing `:root` custom properties | Already well-structured, just needs value updates |
| Status feedback | New notification system | Existing `showStatus()` in `ui.js` | Battle-tested, already handles success/error/info modes with auto-dismiss |
| Reduced motion | Manual animation disabling | Existing `@media (prefers-reduced-motion: reduce)` block | Already in place, just extend for new transitions |
| Cross-platform font rendering | Platform-specific CSS | Existing font stack: `'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif` | Chromium webview handles this uniformly |

**Key insight:** The entire Phase 4 scope is achievable by modifying existing CSS values and migrating a handful of inline styles. No new systems or abstractions needed.

## Common Pitfalls

### Pitfall 1: Removing Spacing Tokens Before Remapping All Usage
**What goes wrong:** Deleting `--space-3` and `--space-5` from `:root` causes `var()` fallback to empty string, breaking layouts silently.
**Why it happens:** These tokens are referenced in component rules throughout `styles.css`.
**How to avoid:** Search for every usage of `--space-3` and `--space-5` across all CSS before removing. The UI-SPEC provides a complete migration guide with per-component remapping.
**Warning signs:** Elements collapsing, unexpected zero padding/margins.

### Pitfall 2: Breaking Wizard Inline Style Display Toggles
**What goes wrong:** Migrating `style="display:none"` to CSS classes breaks JS code that does `element.style.display = ''` to show elements.
**Why it happens:** Confusion between visual styling (migration target) and visibility state (leave alone).
**How to avoid:** Only migrate the 4 wizard inline styles identified in the UI-SPEC migration table. Leave all `display:none` toggle patterns untouched.
**Warning signs:** Wizard sections never appearing, always visible, or flashing.

### Pitfall 3: Forgetting to Update `prefers-reduced-motion` for New Transitions
**What goes wrong:** Users with reduced motion preferences see new animations (card hover transitions, button box-shadow transitions).
**Why it happens:** New transition properties added to components but not covered by the existing `@media` block.
**How to avoid:** After adding any new `transition` property, add a corresponding `transition: none` rule inside the `@media (prefers-reduced-motion: reduce)` block.
**Warning signs:** Motion sensitivity complaints, accessibility audit failures.

### Pitfall 4: Inline Styles Overriding CSS Classes
**What goes wrong:** After migrating JS inline styles to CSS classes, the old `element.style.x` assignments remain in JS, overriding the new CSS class.
**Why it happens:** Forgetting to remove the JS `.style` assignments after adding the CSS class.
**How to avoid:** For each migration in the UI-SPEC table, verify BOTH sides: (1) CSS class added, (2) JS `.style` lines removed and replaced with `className` assignment.
**Warning signs:** Styles not changing despite correct CSS class, or dev tools showing both inline and class-based styles.

### Pitfall 5: Wizard Copy Button Text State Machine
**What goes wrong:** The Copy button says "Copy" initially in HTML, changes to "Copied!" on click, then resets to "Copy URL" after 2 seconds -- inconsistent initial vs. reset text.
**Why it happens:** HTML has `Copy`, JS `setTimeout` resets to `Copy URL`. The initial and reset texts don't match.
**How to avoid:** Change the HTML initial text to "Copy URL" to match the JS reset text. This is a one-line HTML change plus verifying the JS `setTimeout` callback already says `Copy URL`.
**Warning signs:** Button text flickering between different labels.

### Pitfall 6: CSP Violation from Inline Event Handlers
**What goes wrong:** Any new `onclick="..."` attributes in HTML will be blocked by CSP `script-src 'self'`.
**Why it happens:** Temptation to add quick handlers during HTML template changes.
**How to avoid:** All event handlers use `addEventListener` in JS files. Project constraint is enforced by CSP.
**Warning signs:** Console errors about refused inline script execution.

## Code Examples

### Token Value Update (styles.css :root block)
```css
/* Source: 04-UI-SPEC.md Color section */
:root {
  color-scheme: dark;
  --bg-base:        #1e2030;    /* was #1c1d27 */
  --bg-surface:     #242637;    /* was #20222e */
  --bg-elevated:    #2a2d3e;    /* was #252733 */
  --border:         rgba(255,255,255,0.08);   /* was 0.09 */
  --border-row:     rgba(255,255,255,0.05);   /* was 0.07 */
  --accent:         #99222E;    /* UNCHANGED */
  --accent-hover:   #b52a38;    /* UNCHANGED */
  --accent-bg:      rgba(153,34,46,0.80);     /* was 0.85 */
  --accent-glow:    0 0 16px rgba(153,34,46,0.15); /* was 20px/0.2 */
  --text-primary:   #e2e4f0;    /* was #e8e9f0 */
  --text-secondary: #8e92b0;    /* was #8c90aa */
  --text-muted:     #6e7298;    /* was #5c6080 -- significant lift */
  --success:        #34d399;    /* was #22c55e */
  --danger:         #f87171;    /* was #ef4444 */
  --warning:        #fbbf24;    /* was #f59e0b */
  --space-1: 4px;    /* was 0.25rem */
  --space-2: 8px;    /* was 0.5rem */
  /* --space-3 REMOVED (was 0.75rem/12px) */
  --space-4: 16px;   /* was 1rem */
  /* --space-5 REMOVED (was 1.25rem/20px) */
  --space-6: 24px;   /* was 1.5rem */
  --space-8: 32px;   /* was 2rem */
  --radius-sm: 6px;  /* was 4px */
  --radius-md: 8px;  /* was 6px */
  --radius-lg: 12px; /* was 10px */
  --shadow-sm:   0 1px 2px rgba(0,0,0,0.25);  /* was 3px/0.4 */
  --shadow-md:   0 4px 12px rgba(0,0,0,0.35);  /* was 16px/0.5 */
  --shadow-glow: var(--accent-glow);
}
```

### Inline Style Migration Example (settings.js renderHandlers)
```javascript
/* Source: 04-UI-SPEC.md Inline Style Migration table */

/* BEFORE (settings.js:182-186) */
const info = document.createElement('div');
info.className = 'setting-label';
info.style.display = 'flex';
info.style.alignItems = 'center';
info.style.gap = '10px';

/* AFTER */
const info = document.createElement('div');
info.className = 'handler-info';

/* Corresponding CSS class (styles.css) */
/* .handler-info { display: flex; align-items: center; gap: var(--space-2); } */
```

### UI-02 Feedback Fix Example (wizard.js removeSource)
```javascript
/* Source: 04-UI-SPEC.md UI-02 Action Feedback Audit */

/* BEFORE (wizard.js:564-579) -- no success feedback */
async function removeSource(mountId) {
  // ... remove logic ...
  state.addedSources = state.addedSources.filter(s => s.mountId !== mountId);
  renderAddedSources();
  updateGetStartedBtn();
}

/* AFTER -- add showStatus call */
async function removeSource(mountId) {
  // ... remove logic ...
  state.addedSources = state.addedSources.filter(s => s.mountId !== mountId);
  renderAddedSources();
  updateGetStartedBtn();
  showStatus('Source removed', 'success');
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hard-coded color values | CSS custom property tokens | Already in place | Phase 4 leverages this -- change token values, components update automatically |
| Mixed pixel sizes (11, 11.5, 12, 12.5, 13, 16, 18) | 4-tier type scale (11, 13, 16, 32) | Phase 4 | Consolidates 7 sizes to 4 for consistent hierarchy |
| Non-standard spacing (12px, 20px) | 4px-base scale {4, 8, 16, 24, 32} | Phase 4 | Removes `--space-3` and `--space-5`, all values are multiples of 4 in standard set |
| Inline styles in JS render functions | CSS classes with token references | Phase 4 | Eliminates style-logic coupling, makes token system authoritative |

**Deprecated/outdated:**
- `--space-3` (12px): Removed. All 8 usage sites remapped per UI-SPEC migration guide.
- `--space-5` (20px): Removed. All 4 usage sites remapped per UI-SPEC migration guide.
- `font-weight: 500`: Collapsed. Body uses 400, headings use 600. No intermediate weight.

## Inline Style Inventory

Complete inventory of inline styles that ARE and ARE NOT migration targets.

### Migration Targets (from UI-SPEC)
| File | Location | Current | Target CSS |
|------|----------|---------|------------|
| `settings.js:184-186` | `renderHandlers()` info div | `display:flex; alignItems:center; gap:10px` | `.handler-info` class |
| `settings.js:277-279` | `renderOfflinePins()` fileCount span | `fontSize:11px; color:var(--text-muted); marginLeft:4px` | `.pin-file-count` class |
| `settings.js:332` | `renderDashboard()` auth banner icon | `color: 'var(--warning)'` | `.auth-banner-icon` class |
| `wizard.html:56` | Signing-in step spinner row | `display:flex; align-items:center; gap:10px; margin-bottom:20px` | `.auth-status-row` class |
| `wizard.html:64` | Hint paragraph | `margin-top:8px` | Update existing `.hint` rule |
| `wizard.html:66` | Cancel button | `margin-top:20px` | `.cancel-link` class |
| `wizard.html:116` | Actions container | `margin-top:var(--space-6); display:flex; flex-direction:column; align-items:center; gap:var(--space-2)` | `.wizard-actions` class |
| `wizard.html:126` | Done mount list | `list-style:none; width:100%; margin-bottom:22px` | `.done-mount-list` class |

### NOT Migration Targets (display toggles -- leave alone)
| File | Element | Reason |
|------|---------|--------|
| `settings.html:52` | `#auth-banner` `style="display: none;"` | JS toggles via `.style.display = ''` / `= 'none'` |
| `settings.html:55-56` | `#upload-summary`, `#upload-detail` | JS toggles visibility |
| `settings.html:96` | `#nav-pane-field` | JS toggles for Windows-only |
| `wizard.html:39` | `#wizard-footer` | JS toggles visibility |
| `wizard.html:65` | `#auth-error` | JS toggles visibility |
| `wizard.html:71` | `.section-heading` `style="margin-top:0"` | Override to prevent double margin on first heading |
| `wizard.html:77,93,98-101,104,109,115` | Various `display:none` sections | JS toggles visibility |

## UI-02 Action Feedback Audit Summary

Two gaps identified, both in `wizard.js`:

| Gap | Fix |
|-----|-----|
| Copy URL button: HTML says "Copy", JS resets to "Copy URL" | Change `wizard.html` line 62: `>Copy<` to `>Copy URL<` |
| Remove source: no success feedback | Add `showStatus('Source removed', 'success')` after line 578 in `wizard.js` |

All other actions already have appropriate feedback per the UI-SPEC audit table.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Manual visual inspection (frontend CSS/JS changes) |
| Config file | N/A -- no automated visual regression tooling |
| Quick run command | `make build` (ensures Rust compilation still passes -- no Rust changes expected) |
| Full suite command | `make test` (integration tests in `crates/carminedesktop-app/tests/`) |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UI-01 | Visual design modernized, consistent styling | manual-only | Manual: open settings.html and wizard.html in Tauri webview, verify visual changes | N/A |
| UI-02 | All actions provide feedback | manual-only | Manual: trigger each action from UI-SPEC audit table, verify `showStatus()` fires | N/A |

**Justification for manual-only:** Phase 4 changes are exclusively CSS token values, CSS rules, and 2 minor JS fixes. There are no behavioral changes to Rust code, no API changes, no state logic changes. The existing integration tests verify backend command registration and IPC. Visual changes require human visual inspection -- CSS rendering cannot be meaningfully unit-tested without a visual regression framework (which is not in the project stack and would violate the zero-dependency convention).

### Sampling Rate
- **Per task commit:** `make build` -- verify no Rust compilation errors
- **Per wave merge:** `make clippy && make test` -- verify CI passes
- **Phase gate:** Manual visual inspection of both settings.html and wizard.html in Tauri webview on the host machine

### Wave 0 Gaps
None -- no test infrastructure changes needed. Phase 4 is CSS/JS only with no new test files required. CI (`make clippy`, `make test`) validates that no Rust code was accidentally broken.

## Open Questions

1. **Semantic color contrast against new backgrounds**
   - What we know: UI-SPEC specifies new semantic colors (`--success: #34d399`, `--danger: #f87171`, `--warning: #fbbf24`) chosen for warmth against the lifted dark palette.
   - What's unclear: Whether the new semantic badge background opacities (e.g., `rgba(34,197,94,0.12)` for success badges) need adjustment since the base colors changed.
   - Recommendation: Use the UI-SPEC values as-is. The badge backgrounds reference the *old* RGB values in their `rgba()` definitions -- these should be updated to match the new semantic token RGB values for consistency. Verify visually during implementation.

2. **wizard.html section-heading `style="margin-top:0"`**
   - What we know: Line 71 has `style="margin-top:0"` to override the `.section-heading` margin for the first heading in the sources step.
   - What's unclear: Whether to migrate this to a CSS utility class or leave as inline.
   - Recommendation: Leave as inline -- it's a one-off override on a specific element, not a reusable pattern. Alternatively, rely on the existing `.section-heading:first-child { margin-top: 0; }` rule if this heading is indeed a first-child in its container.

## Sources

### Primary (HIGH confidence)
- `04-UI-SPEC.md` -- Complete design contract with exact token values, component specs, migration tables, and feedback audit
- `04-CONTEXT.md` -- User decisions constraining the design direction
- `styles.css` -- Current token values and component rules (703 lines, fully read)
- `settings.js` -- Current render functions and event handling (1038 lines, fully read)
- `wizard.js` -- Current wizard flow and event handling (736 lines, fully read)
- `settings.html` -- Current dashboard/settings HTML structure (247 lines, fully read)
- `wizard.html` -- Current wizard HTML structure (138 lines, fully read)
- `ui.js` -- showStatus/formatError utilities (46 lines, fully read)

### Secondary (MEDIUM confidence)
- `CLAUDE.md` -- Project conventions (CSP constraint, no inline handlers, zero warnings CI)
- `03-CONTEXT.md` -- Phase 3 dashboard decisions to preserve

### Tertiary (LOW confidence)
- None -- all findings based on direct code inspection

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- directly inspected all 6 target files, zero new dependencies
- Architecture: HIGH -- existing token cascade pattern is well-understood and verified in code
- Pitfalls: HIGH -- derived from actual code patterns found during inline style inventory
- UI-SPEC accuracy: HIGH -- cross-verified every UI-SPEC claim against actual code (token values, inline styles, button text, feedback gaps)

**Research date:** 2026-03-19
**Valid until:** Indefinite -- no external dependencies that could change
