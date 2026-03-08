---
run: run-cloud-mount-003
work_items: [design-system-setup, wizard-dark-redesign, settings-dark-redesign]
intent: ui-dark-premium-redesign
generated: 2026-03-08T12:32:30Z
mode: confirm
scope: wide
---

# Implementation Walkthrough: Dark Premium UI Redesign

## Summary

The CloudMount UI was redesigned from a generic light-themed form interface to a cohesive dark premium design system inspired by Vercel/Linear. A shared `styles.css` was created with the full Violet/Space token set, Inter variable font, and reusable component classes. Both `wizard.html` and `settings.html` had their inline style blocks removed and replaced by the shared design system. A shared `ui.js` was extracted from `settings.js` to provide the `showStatus` notification utility across both surfaces.

## Structure Overview

Three layers work together:

1. **`dist/fonts/InterVariable.woff2`** — self-hosted Inter font, loaded exclusively via `@font-face` in `styles.css`. No CDN.
2. **`dist/styles.css`** — single source of truth for all visual tokens (colors, spacing, radius, shadows), the `@font-face` declaration, CSS reset, and reusable component classes. Consumed by both pages.
3. **`dist/ui.js`** — shared JS utility loaded before both `wizard.js` and `settings.js`. Provides `showStatus(message, type)` for toast/status-bar notifications.

Both HTML pages now load `styles.css` (via `<link>`) and `ui.js` (via `<script>`) before their respective app JS files. The Rust/Tauri backend and all Tauri command wiring is untouched — only frontend assets changed.

## Files Changed

### Created

| File | Purpose |
|------|---------|
| `crates/cloudmount-app/dist/fonts/InterVariable.woff2` | Inter variable font v4.1 (344 KB), self-hosted in dist/ |
| `crates/cloudmount-app/dist/styles.css` | Full design system: tokens, reset, @font-face, all component classes |
| `crates/cloudmount-app/dist/ui.js` | Shared `showStatus(message, type)` utility (extracted from settings.js) |

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Removed inline `<style>` block; added `<link>` + `<script src="ui.js">`; `.container` → `.wizard-container`; welcome step wrapped in `.welcome-hero` with radial glow; buttons use `.btn-secondary` where appropriate |
| `crates/cloudmount-app/dist/settings.html` | Removed inline `<style>` block; added `<link>` + `<script src="ui.js">`; Sign Out and Clear Cache updated to `.btn-danger`; mount list gets `settings-mounts` class |
| `crates/cloudmount-app/dist/settings.js` | Removed `showStatus` fn + `_statusTimer` var; `removeBtn.className` → `'btn-danger'`; `clearCache` querySelector → `.btn-danger`; `actions.className = 'mount-actions'` added |

## Key Implementation Details

### 1. CSS Custom Property Token System

All visual values flow from `:root` CSS custom properties. The token table covers backgrounds (`--bg-base`, `--bg-surface`, `--bg-elevated`), borders, accent colors, text hierarchy, semantic states (success/danger), spacing scale, border radii, and box shadows. Component classes reference these tokens exclusively — no hardcoded hex values appear in any component rule.

### 2. Inter Variable Font — Self-Hosted

The Inter v4.1 `InterVariable.woff2` (from `web/` inside the official GitHub release zip) is placed in `dist/fonts/` and referenced via `@font-face` in `styles.css`. The `font-weight: 100 900` range declaration covers the full weight axis with a single file. Tauri serves all `dist/` assets locally, satisfying the CSP `default-src 'self'` restriction.

### 3. CSS Specificity Strategy

The global `button` selector (type + pseudo-class specificity `(0,2,1)`) is more specific than plain class selectors like `.btn-secondary` `(0,2,0)`. High-specificity variants (`.btn-secondary`, `.btn-danger`) use `:hover:not(:disabled)` in their selectors which reaches `(0,3,0)` — high enough to override the global button rule. Two edge-case selectors (`.sp-back-link:hover` and `.btn-remove:hover`) needed `button.` prefixed to reach `(0,2,1)` parity and win by source order.

### 4. `showStatus` Extraction

The `showStatus` function in `settings.js` was identical to what `ui.js` now provides. The only behavioral extension: `ui.js` also auto-dismisses `info` type (the original only auto-dismissed `success`). The `#status-bar` element remains in `settings.html` as the mount point — `showStatus` targets it by ID, consistent with the original implementation.

### 5. Mount Item Structure in Settings

Mount items are created dynamically by `loadMounts()` in `settings.js`. The `actions` container div was missing a class, meaning the `.mount-actions { gap }` rule never applied. Adding `actions.className = 'mount-actions'` corrects button spacing without changing any behavioral logic.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Font variant | Variable font `InterVariable.woff2` (single file) | One file covers all weights vs. multiple static subset files; ~344 KB vs. multiple 140 KB files. Variable font is marginally larger but simpler. |
| Inline style blocks | Removed entirely from both HTML files | `styles.css` covers all cases; keeping both would cause specificity confusion |
| `showStatus` API | Match existing signature `(message, type)` exactly | Zero call-site changes needed in `settings.js`; direct drop-in replacement |
| `#status-bar` element | Kept in `settings.html` | Needed as DOM mount point for `showStatus()`; CSS animation moved to `styles.css` |
| `.btn-remove` style | Outlined (transparent bg, danger border) with fill on hover | Less aggressive than solid red at rest; signals removability without alarming at a glance |
| Wizard container class | Renamed `.container` → `.wizard-container` | More specific, avoids any collision if a `.container` utility is ever added |

## Deviations from Plan

**design-system-setup**: None. All deliverables as planned.

**wizard-dark-redesign**: The plan noted wizard.js might need minor color-related changes (inline `style` mutations). On inspection, `wizard.js` sets no inline colors — it only toggles `display`. No JS changes needed. This is a positive deviation (less work).

**settings-dark-redesign**: The plan listed `settings.js` fix for `clearCache` querySelector (`button.danger` → `.btn-danger`). This was correctly executed. Additionally, the `createTextNode(' ')` spacer between toggle/remove buttons was removed and replaced by CSS gap via `mount-actions` class — a minor cleanup caught during code review.

## Dependencies Added

| Asset | Source | Why |
|-------|--------|-----|
| `dist/fonts/InterVariable.woff2` | Inter v4.1 GitHub release (`web/InterVariable.woff2` from zip) | Self-hosted Inter variable font — CSP compliance, no CDN |

No new npm/Cargo dependencies. All frontend assets are vanilla HTML/CSS/JS.

## How to Verify

1. **Build passes**
   ```bash
   cargo build -p cloudmount-app
   ```
   Expected: `Finished` with no warnings or errors.

2. **Font file present and non-empty**
   ```bash
   ls -lh crates/cloudmount-app/dist/fonts/
   ```
   Expected: `InterVariable.woff2` ~344 KB.

3. **Manual — Wizard appearance**
   Run the app with desktop feature and open the wizard. Expected:
   - Dark `#0e0f14` background
   - "CloudMount" title with subtle violet radial glow
   - Sign-in button is violet (`#7c5cfc`)
   - Inter font rendered (vs. system sans-serif)
   - Signing-in step: dark spinner with violet arc, dark URL box

4. **Manual — Settings appearance**
   Open settings. Expected:
   - Dark surface tab bar with accent underline on active tab
   - All inputs/selects dark with border focus
   - Sign Out button red (`var(--danger)`)
   - Mount items rendered as dark cards with name + path

5. **Manual — Status bar**
   Trigger a settings save. Expected:
   - Green status bar slides up from bottom with "Settings saved"
   - Auto-dismisses after 3 seconds

6. **Manual — Add-mount wizard mode**
   With an existing account (authenticated), open the wizard. Expected:
   - Goes directly to sources step
   - "Get started" button shows as "Close" (existing `wizard.js` behavior)

## Test Coverage

- Acceptance criteria validated: 37 / 37
- Build test: PASS
- No automated unit tests added (frontend-only, vanilla JS — no test harness configured)
- Manual verification is the primary QA path for this change

## Developer Notes

- **CSP `unsafe-inline`**: The `style-src 'self' 'unsafe-inline'` CSP allows inline `style=""` attributes (used for `display:none` toggles throughout the JS). Do NOT remove `'unsafe-inline'` without auditing all JS `.style.xxx` mutations.
- **`flex-basis` beats `width`**: The global `input[type="text"] { width: 100% }` rule might look like it conflicts with `.url-input { flex: 1 }` in the URL box. It doesn't — `flex: 1` expands to `flex-basis: 0%` which takes precedence over `width` for flex items.
- **Selector specificity trap**: The global `button:hover:not(:disabled)` selector reaches `(0,2,1)` due to the `:not()` argument. Any overriding `.my-class:hover` needs to be `button.my-class:hover` or `.my-class:hover:not(:disabled)` to beat it.
- **`showStatus` on wizard**: `ui.js` is loaded in wizard too, so `showStatus` is available if needed in future wizard error flows.
- **Font fallback**: `@font-face` uses `font-display: swap` — the UI renders immediately with the system fallback, then swaps to Inter when loaded. First load may flicker briefly.

---
*Generated by specs.md - fabriqa.ai FIRE Flow Run run-cloud-mount-003*
