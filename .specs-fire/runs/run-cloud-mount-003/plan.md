# Implementation Plan — run-cloud-mount-003

**Intent**: Dark Premium UI Redesign (`ui-dark-premium-redesign`)
**Scope**: wide — 3 work items, all `confirm` mode
**Execution order**: design-system-setup → wizard-dark-redesign → settings-dark-redesign

---

## Work Item 1: `design-system-setup`
### Create shared design system (tokens, Inter font, shared CSS/JS)

### Approach

Create the shared foundation layer — no visual changes to existing pages yet, just the new assets and wiring the references in.

1. Download `InterVariable.woff2` from the Inter v4 GitHub release → `dist/fonts/InterVariable.woff2`
2. Create `dist/styles.css` — full Violet/Space token table, CSS reset, `@font-face`, and all reusable component classes
3. Create `dist/ui.js` — exports `showStatus(message, type)` (extracted from `settings.js`)
4. Add `<link rel="stylesheet" href="styles.css">` to both `wizard.html` and `settings.html`
5. Add `<script src="ui.js"></script>` before `wizard.js` in `wizard.html` and before `settings.js` in `settings.html`
6. In `settings.js`: remove the `showStatus` function + `_statusTimer` var; all call-sites already match the extracted API

### Files to Create

| File | Purpose |
|------|---------|
| `crates/cloudmount-app/dist/fonts/InterVariable.woff2` | Inter variable font (self-hosted) |
| `crates/cloudmount-app/dist/styles.css` | Design tokens, reset, `@font-face`, component classes |
| `crates/cloudmount-app/dist/ui.js` | Shared `showStatus(message, type)` utility |

### Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Add `<link>` + `<script src="ui.js">` before `wizard.js` |
| `crates/cloudmount-app/dist/settings.html` | Add `<link>` + `<script src="ui.js">` before `settings.js` |
| `crates/cloudmount-app/dist/settings.js` | Remove `showStatus` fn + `_statusTimer` var (extracted to `ui.js`) |

### Component Classes in `styles.css`

`.btn`, `.btn-secondary`, `.btn-danger`, `.btn-sm`, `.input`, `.card`, `.spinner` (dark), `.badge`, `.tabs`/`.tab`, `.section-heading`, `.status-bar`

---

## Work Item 2: `wizard-dark-redesign`
### Apply dark premium design to wizard

### Approach

Full visual pass on `wizard.html`. Remove the entire `<style>` block (all styling moves to `styles.css`). Apply design-system classes to all four steps.

1. Remove the `<style>` block from `wizard.html`
2. Add dark-specific CSS to `styles.css` for wizard-specific layouts (`.url-box`, `.url-input`, `.sources-spinner`, `.sp-*` classes, `.source-card`, `.added-source-row`)
3. Update HTML markup to use design-system classes and CSS variables:
   - **Welcome step**: wrap in `.welcome-hero` container with violet glow `::before` pseudo-element
   - **Signing-in step**: spinner uses `--accent` top-color, `.url-input` uses `--bg-elevated`, buttons updated
   - **Sources step**: `.source-card` uses `--bg-surface`, `.sp-result-row` dark cards, `.section-heading` muted uppercase
   - **Success step**: `.mount-item` as small `.card`, close button uses `.btn`
4. `wizard.js` needs only one cosmetic change: `errEl.style.color` inline mutations replaced with CSS class — actually wizard.js doesn't set inline colors; only `style.display` manipulation. No JS changes needed.

### Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.html` | Remove `<style>` block; update HTML to use design-system classes |
| `crates/cloudmount-app/dist/styles.css` | Append wizard-specific component styles (not duplicating what's already there) |

### No changes to `wizard.js` — all IDs and event wiring remain intact.

---

## Work Item 3: `settings-dark-redesign`
### Apply dark premium design to settings

### Approach

Full visual pass on `settings.html`. Remove the `<style>` block. Update all panels.

1. Remove the `<style>` block from `settings.html`
2. Add settings-specific CSS to `styles.css` (tab bar, panels, mount list, form fields, status bar animation)
3. Update HTML markup:
   - **Tab bar**: `.tabs`/`.tab` from design system, `--accent` active underline
   - **General**: `.field` labels with `--text-secondary`, `<select>` uses `.input`, Save uses `.btn`
   - **Mounts**: `.mount-item` uses `.card`, mount name `--text-primary`, path `--text-secondary`, buttons styled
   - **Account**: email `--text-primary`, Sign Out uses `.btn-danger`
   - **Advanced**: all inputs/selects use `.input`, Save uses `.btn`, Clear Cache uses `.btn-danger`
   - **`#status-bar`**: kept as mount point for `showStatus()`, but CSS now lives in `styles.css`
4. In `settings.js`: change `removeBtn.className = 'danger'` → `removeBtn.className = 'btn-danger'` to match design system

### Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/settings.html` | Remove `<style>` block; update HTML to use design-system classes |
| `crates/cloudmount-app/dist/styles.css` | Append settings-specific component styles |
| `crates/cloudmount-app/dist/settings.js` | `removeBtn.className = 'danger'` → `'btn-danger'` |

---

## Tests

This is a frontend-only change (HTML/CSS/JS in `dist/`). No Rust code changes.

**Validation**:
- `cargo build -p cloudmount-app` passes (build.rs picks up dist/ files)
- Visual inspection of both pages
- All IDs referenced in `wizard.js` / `settings.js` remain present in updated HTML

---

## Notes

- Font download: will use `curl` to fetch InterVariable.woff2 from the Inter v4.1 GitHub release
- CSP `style-src 'self' 'unsafe-inline'` allows the existing inline `style=""` attributes on elements (display:none toggles) — these are fine to keep
- `wizard.js` "Get started → Close" in add-mount mode is already implemented in JS (lines 107–112); only the cosmetic presentation changes here
- The `#status-bar` `<div>` remains in `settings.html` as the mount point for `showStatus()`
