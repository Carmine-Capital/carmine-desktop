# UI/UX Refonte Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refonte complète de l'UI Carmine Desktop (wizard + settings) vers un style compact/minimaliste inspiré Linear/Arc/Handy, en deux phases.

**Architecture:** Phase 1 rewrites HTML and CSS for both views plus minimal JS changes (selector updates, auto-save). Phase 2 refactors JS into centralized state + declarative rendering. Both views share the same sidebar layout — settings uses nav tabs, wizard uses a stepper.

**Tech Stack:** Vanilla HTML/CSS/JS, Tauri IPC (`window.__TAURI__.core.invoke`), Inter font, CSS custom properties.

**Spec:** `docs/superpowers/specs/2026-03-15-ui-ux-refonte-design.md`

---

## Chunk 1: Phase 1 — Design System & Settings

### File Structure (Phase 1)

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/carminedesktop-app/dist/styles.css` | Rewrite | New design system tokens, all component styles |
| `crates/carminedesktop-app/dist/settings.html` | Rewrite | New sidebar layout, flat rows, no settings-group cards |
| `crates/carminedesktop-app/dist/settings.js` | Modify | Update selectors, add auto-save, remove dirty tracking |
| `crates/carminedesktop-app/dist/wizard.html` | Rewrite | Sidebar stepper layout, restructured steps |
| `crates/carminedesktop-app/dist/wizard.js` | Modify | Update selectors to match new HTML IDs/classes |
| `crates/carminedesktop-app/dist/ui.js` | No change | `showStatus()` and `formatError()` unchanged |

---

### Task 1: Rewrite CSS Design System

**Files:**
- Rewrite: `crates/carminedesktop-app/dist/styles.css`

- [ ] **Step 1: Write the new design tokens (`:root`)**

Replace the existing `:root` block with updated tokens:

```css
:root {
  color-scheme: dark;

  /* Backgrounds — lifted one notch */
  --bg-base:        #121318;
  --bg-surface:     #151620;
  --bg-elevated:    #1a1b24;
  --border:         rgba(255,255,255,0.04);
  --border-row:     rgba(255,255,255,0.03);

  /* Accent — Carmine Red (unchanged) */
  --accent:         #99222E;
  --accent-hover:   #b52a38;
  --accent-bg:      rgba(153,34,46,0.85);
  --accent-glow:    0 0 20px rgba(153, 34, 46, 0.2);

  /* Text — attenuated */
  --text-primary:   #d4d5de;
  --text-secondary: #6b6f85;
  --text-muted:     #3d3f54;

  /* Semantic */
  --success:        #22c55e;
  --danger:         #ef4444;

  /* Spacing */
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-5: 1.25rem;
  --space-6: 1.5rem;
  --space-8: 2rem;

  /* Radius */
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 10px;

  /* Shadows */
  --shadow-sm:   0 1px 3px rgba(0, 0, 0, 0.4);
  --shadow-md:   0 4px 16px rgba(0, 0, 0, 0.5);
  --shadow-glow: var(--accent-glow);
}
```

- [ ] **Step 2: Write reset, typography, and animation base**

Keep the existing reset and `@font-face` declaration for Inter. Update `body` to use `--bg-base` and `--text-primary`. Keep `@keyframes spin`.

- [ ] **Step 3: Write button component styles**

New button styles — compact, 4 variants:

```css
/* Base button */
button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  background: var(--accent-bg);
  color: #fff;
  border: none;
  padding: 7px 18px;
  border-radius: var(--radius-md);
  font-size: 12.5px;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
  transition: background 0.15s ease, opacity 0.15s ease;
  white-space: nowrap;
}
button:hover:not(:disabled) { background: var(--accent-hover); }
button:disabled { opacity: 0.4; cursor: not-allowed; }
button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

/* Ghost / Secondary */
.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
  border: 1px solid rgba(255,255,255,0.05);
}
.btn-ghost:hover:not(:disabled) {
  background: var(--bg-elevated);
  color: var(--text-primary);
}

/* Destructive text */
.btn-danger {
  background: none;
  color: var(--accent);
  border: none;
  padding: 0;
  font-size: 11px;
}
.btn-danger:hover:not(:disabled) { color: var(--accent-hover); background: none; }

/* Text link */
.btn-link {
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 12px;
  padding: 0;
  cursor: pointer;
  font-family: inherit;
}
.btn-link:hover:not(:disabled) { color: var(--text-primary); background: none; }

/* Small */
.btn-sm { padding: var(--space-1) var(--space-3); font-size: 11px; }

/* Icon button */
.btn-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-muted);
  padding: var(--space-1);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: color 0.15s ease;
}
.btn-icon:hover:not(:disabled) { color: var(--text-secondary); }
.btn-icon.btn-icon-danger:hover:not(:disabled) { color: var(--danger); }
```

- [ ] **Step 4: Write input, select, toggle styles**

```css
/* Inputs & Selects — compact */
input[type="text"], input[type="number"], select, .input {
  background: var(--bg-elevated);
  color: var(--text-primary);
  border: 1px solid rgba(255,255,255,0.05);
  border-radius: 5px;
  padding: 4px 10px;
  font-size: 11.5px;
  font-family: inherit;
  outline: none;
  transition: border-color 0.15s ease;
  appearance: none;
  -webkit-appearance: none;
}
input:focus, select:focus { border-color: var(--accent); }
select {
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%236b6f85' d='M2 4l4 4 4-4'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0.5rem center;
  padding-right: 1.5rem;
}
::placeholder { color: var(--text-muted); }

/* Toggle — compact 30x16 */
.toggle-switch { position: relative; display: inline-flex; align-items: center; cursor: pointer; }
.toggle-switch input[type="checkbox"] { position: absolute; opacity: 0; width: 0; height: 0; }
.toggle-track {
  width: 30px; height: 16px;
  background: var(--bg-elevated);
  border-radius: 8px;
  position: relative;
  transition: background 0.2s ease;
  flex-shrink: 0;
}
.toggle-track::after {
  content: ''; position: absolute;
  top: 2px; left: 2px;
  width: 12px; height: 12px;
  background: var(--text-muted);
  border-radius: 50%;
  transition: transform 0.2s ease, background 0.2s ease;
}
.toggle-switch input:checked + .toggle-track { background: var(--accent); }
.toggle-switch input:checked + .toggle-track::after { transform: translateX(14px); background: #fff; }
.toggle-switch input:focus-visible + .toggle-track { outline: 2px solid var(--accent); outline-offset: 2px; }
```

- [ ] **Step 5: Write layout styles (sidebar + main content)**

```css
/* App Layout */
.app-layout { display: flex; height: 100vh; }

/* Sidebar — 190px */
.sidebar {
  width: 190px;
  background: var(--bg-surface);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  padding: 14px 10px;
}

.sidebar-header {
  padding: 4px 8px 18px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.sidebar-logo {
  width: 22px; height: 22px;
  background: linear-gradient(135deg, #99222E, #c4354a);
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  color: #fff;
  font-weight: 700;
  flex-shrink: 0;
}
.sidebar-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

/* Nav items */
.sidebar-nav { display: flex; flex-direction: column; gap: 2px; }
.nav-item {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 12px;
  border-radius: var(--radius-md);
  color: var(--text-muted);
  background: transparent;
  border: none; cursor: pointer;
  font-size: 12.5px; font-weight: 500; font-family: inherit;
  text-align: left; width: 100%;
  transition: background 0.15s ease, color 0.15s ease;
}
.nav-item svg { flex-shrink: 0; }
.nav-item:hover:not(.active):not(:disabled) { color: var(--text-secondary); }
.nav-item.active { background: var(--accent-bg); color: #fff; }
.nav-item:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

/* Sidebar footer */
.sidebar-footer {
  margin-top: auto;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
}
.sidebar-footer .account-email {
  font-size: 11px; color: var(--text-muted);
  margin-bottom: 4px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.sidebar-footer .sign-out-link { font-size: 11px; color: var(--accent); cursor: pointer; }

/* Main content */
.main-content { flex: 1; overflow-y: auto; padding: 22px 28px; }
```

- [ ] **Step 6: Write setting-row and section-heading styles**

```css
/* Section heading */
.section-heading {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  margin: 22px 0 14px;
}
.section-heading:first-child { margin-top: 0; }

/* Setting row */
.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 0;
  border-bottom: 1px solid var(--border-row);
}
.setting-row:last-child { border-bottom: none; }
.setting-label { flex: 1; min-width: 0; }
.setting-label .label-text { font-size: 13px; color: var(--text-primary); }
.setting-label .label-sub { font-size: 11px; color: var(--text-muted); margin-top: 1px; }
.setting-control { flex-shrink: 0; display: flex; align-items: center; gap: 8px; }
```

- [ ] **Step 7: Write panel, mount-list, handler-list, and status-bar styles**

```css
/* Panel show/hide */
.panel { display: none; }
.panel.active { display: block; }

/* Mount rows */
.mount-info { flex: 1; min-width: 0; }
.mount-name { font-size: 13px; color: var(--text-primary); }
.mount-path { font-size: 11px; color: var(--text-muted); margin-top: 1px; }
.mount-actions { display: flex; align-items: center; gap: 12px; }
.setting-row.mount-disabled .mount-name { color: var(--text-secondary); }
.setting-row.mount-disabled .mount-path { color: var(--text-muted); }
.mount-empty { color: var(--text-muted); font-style: italic; padding: 9px 0; }

/* Handler rows */
.handler-ext { font-size: 12.5px; font-weight: 600; color: var(--text-primary); min-width: 3rem; }
.handler-name { font-size: 11.5px; color: var(--text-muted); }
.handler-actions { display: flex; align-items: center; gap: var(--space-2); flex-shrink: 0; }
.handler-override-input { width: 10rem; font-size: 11px; padding: var(--space-1) var(--space-2); }
.handler-empty { color: var(--text-muted); font-style: italic; padding: 9px 0; }

/* Status bar */
#status-bar {
  position: fixed; bottom: 0; left: 0; right: 0;
  padding: var(--space-2) var(--space-4);
  font-size: 12px;
  transform: translateY(100%);
  transition: transform 0.2s ease, opacity 0.2s ease;
  opacity: 0; z-index: 100;
}
#status-bar.visible { transform: translateY(0); opacity: 1; display: flex; align-items: center; gap: 0.5rem; }
#status-bar.visible.success { background: var(--success); color: #000; }
#status-bar.visible.error { background: var(--danger); color: #fff; }
#status-bar.visible.info { background: var(--bg-elevated); color: var(--text-primary); border-top: 1px solid var(--border); }
.status-dismiss { background: none; border: none; cursor: pointer; color: inherit; font-size: 1rem; padding: 0; margin-left: auto; }

/* Spinner */
.spinner {
  display: inline-block; width: 20px; height: 20px;
  border: 2px solid var(--bg-elevated);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
```

- [ ] **Step 8: Write wizard-specific styles (stepper, steps)**

```css
/* Wizard stepper */
.stepper-label {
  font-size: 11px; font-weight: 600; color: var(--text-muted);
  text-transform: uppercase; letter-spacing: 0.08em;
  padding: 0 12px 10px;
}
.stepper-nav { display: flex; flex-direction: column; gap: 2px; }
.stepper-item {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 12px;
  font-size: 12.5px; font-weight: 500;
  color: var(--text-muted);
  border-radius: var(--radius-md);
}
.stepper-item.active { background: var(--accent-bg); color: #fff; }
.stepper-item.done { color: var(--text-secondary); }
.step-number {
  width: 18px; height: 18px;
  border: 1px solid rgba(255,255,255,0.08);
  border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  font-size: 10px; flex-shrink: 0;
}
.stepper-item.active .step-number { background: rgba(255,255,255,0.15); border-color: transparent; font-weight: 600; }
.stepper-item.done .step-number { background: var(--success); border-color: transparent; color: #fff; }

/* Wizard step content */
.step { display: none; }
.step.active { display: flex; flex-direction: column; }

/* Wizard content centering (for steps 1, 2, 4) */
.step-centered { justify-content: center; min-height: calc(100vh - 44px); padding: 0 28px; }

/* Step 3 uses scrolling layout */
.step-scroll { padding: 22px 28px; }

.step h1 { font-size: 16px; font-weight: 600; color: var(--text-primary); margin-bottom: 6px; }
.step .step-sub { font-size: 13px; color: var(--text-muted); margin-bottom: 22px; max-width: 320px; line-height: 1.5; }
.step h1.step-title-lg { font-size: 18px; }

/* Auth URL fallback */
.url-row { display: flex; align-items: center; gap: 8px; width: 100%; max-width: 380px; }
.url-input {
  flex: 1; min-width: 0;
  font-size: 11px; color: var(--text-muted);
  background: var(--bg-elevated);
  padding: 6px 10px;
  border-radius: 5px;
  border: 1px solid rgba(255,255,255,0.05);
  font-family: monospace;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.auth-countdown { font-size: 12px; color: var(--text-secondary); margin: var(--space-2) 0; }
.auth-countdown.warning { color: #f59e0b; font-weight: 600; }

/* SharePoint rows */
.sp-result-row {
  padding: 9px 0;
  border-bottom: 1px solid var(--border-row);
  cursor: pointer;
  text-align: left;
  color: var(--text-primary);
  font-size: 13px;
}
.sp-result-row:hover { color: var(--accent); }
.sp-result-url { font-size: 11px; color: var(--text-muted); margin-top: 1px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* Library rows with checkboxes */
.lib-row {
  display: flex; align-items: center; gap: 10px;
  padding: 9px 0;
  border-bottom: 1px solid var(--border-row);
  cursor: pointer;
}
.lib-row:hover:not(.mounted) { background: transparent; }
.lib-check {
  width: 16px; height: 16px;
  border: 1.5px solid rgba(255,255,255,0.08);
  border-radius: 4px;
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0;
  font-size: 10px; color: transparent;
  transition: all 0.15s ease;
}
.lib-row.selected .lib-check { border-color: var(--accent); background: var(--accent); color: #fff; }
.lib-row.mounted { opacity: 0.4; cursor: default; }
.lib-row.mounted .lib-check { border-color: var(--success); background: var(--success); color: #fff; }
.lib-info { flex: 1; }
.lib-name { font-size: 13px; color: var(--text-primary); }
.lib-badge { font-size: 10.5px; color: var(--text-muted); margin-top: 1px; }
.lib-row.mounted .lib-badge { color: var(--success); }

/* Added source rows */
.added-source-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid var(--border-row);
}
.added-source-name { font-size: 13px; color: var(--text-primary); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* Success hint */
.hint { font-size: 11px; color: var(--text-muted); margin-top: 10px; }

/* Error message */
.error-msg { color: var(--danger); font-size: 12px; margin-top: var(--space-2); }

/* Search row */
.sp-search-row { display: flex; gap: var(--space-2); margin-bottom: var(--space-4); width: 100%; }
.sp-search-row input[type="text"] { flex: 1; width: auto; }

/* Attribution */
.attribution { font-size: 11px; color: var(--text-secondary); line-height: 1.6; }
.attribution a { color: var(--accent); text-decoration: none; }
.attribution a:hover { text-decoration: underline; }
```

- [ ] **Step 9: Verify the CSS compiles cleanly**

Open `styles.css` in the browser via the Tauri dev mode or directly to check for syntax errors. Verify with the settings page that fonts load.

- [ ] **Step 10: Commit**

```bash
git add crates/carminedesktop-app/dist/styles.css
git commit -m "feat(ui): rewrite CSS design system — compact Linear/Handy-inspired tokens and components"
```

---

### Task 2: Rewrite Settings HTML

**Files:**
- Rewrite: `crates/carminedesktop-app/dist/settings.html`

- [ ] **Step 1: Write the settings HTML with new sidebar layout**

Replace the entire `settings.html` with the new structure. Key changes:
- Sidebar 190px with logo: `<div class="sidebar-header"><div class="sidebar-logo">C</div><span class="sidebar-title">Carmine</span></div>`
- Nav items with 15px SVG icons (same feather icons as current, just scaled to 15px)
- Footer with account email + sign out link: `<div class="sidebar-footer"><p class="account-email" id="account-email">Not signed in</p><button id="sign-out-btn" class="btn-danger">Sign Out</button></div>`
- Panel General: flat rows with `.setting-row`, section headings, no `.settings-group` cards
- Panel Mounts: empty `<ul id="mount-list">` (populated by JS)
- Panel About: simple rows
- No Save button, no unsaved badge
- Advanced section visible (no `<details>` collapsible)

Preserve all existing element IDs that JS references:
- `auto-start`, `notifications`, `explorer-nav-pane`, `sync-interval`
- `cache-dir`, `cache-max-size`, `metadata-ttl`, `log-level`
- `handler-list`, `btn-redetect`
- `mount-list`, `btn-add-mount`
- `sign-out-btn`, `account-email`
- `nav-pane-field`
- `status-bar`
- `panel-general`, `panel-mounts`, `panel-about`
- `tab-general`, `tab-mounts`, `tab-about`

Remove: `save-settings` button, `unsaved-badge` div, `<details class="advanced-details">` wrapper.

The `btn-clear-cache` moves inline inside the cache size `.setting-row`:
```html
<div class="setting-row">
  <div class="setting-label">
    <div class="label-text">Cache size limit</div>
    <div class="label-sub">Where downloaded files are stored</div>
  </div>
  <div class="setting-control">
    <input type="text" id="cache-max-size" placeholder="5GB">
    <button id="btn-clear-cache" class="btn-danger">Clear</button>
  </div>
</div>
```

Each `.setting-row` in General panel follows this pattern:
```html
<div class="setting-row">
  <div class="setting-label">
    <div class="label-text">Start on login</div>
    <div class="label-sub">Launch Carmine when you sign in</div>
  </div>
  <div class="setting-control">
    <label class="toggle-switch">
      <input type="checkbox" id="auto-start">
      <span class="toggle-track"></span>
    </label>
  </div>
</div>
```

ARIA: Preserve `role="tablist"` on `<nav>`, `role="tab"` on nav items, `role="tabpanel"` on panels.

- [ ] **Step 2: Verify HTML is valid and IDs match JS expectations**

Cross-reference every `getElementById` and `querySelector` call in `settings.js` against the new HTML IDs.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.html
git commit -m "feat(ui): rewrite settings.html — flat rows, compact sidebar, no cards"
```

---

### Task 3: Update Settings JS (auto-save + selector fixes)

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.js`

- [ ] **Step 1: Remove dirty tracking code**

Delete: `_savedValues` variable, `snapshotValues()` function, `checkDirty()` function, and all calls to them.

- [ ] **Step 2: Remove Save button references**

Delete: the `save-settings` event listener line, the `btn.disabled` / `btn.textContent` manipulation in `saveSettings()`.

- [ ] **Step 3: Modify `saveSettings()` for auto-save (silent success)**

Remove the `showStatus('Settings saved', 'success')` call. Keep only the error path. Remove the Save button DOM manipulation. The function should just call `invoke('save_settings', {...})` and only `showStatus` on error.

```js
async function saveSettings() {
  const syncInterval = parseInt(document.getElementById('sync-interval').value);
  if (isNaN(syncInterval) || syncInterval <= 0) {
    showStatus('Sync interval must be a positive number', 'error');
    return;
  }
  const metadataTtl = parseInt(document.getElementById('metadata-ttl').value);
  if (isNaN(metadataTtl) || metadataTtl <= 0) {
    showStatus('Metadata TTL must be a positive number', 'error');
    return;
  }
  try {
    await invoke('save_settings', {
      autoStart: document.getElementById('auto-start').checked,
      notifications: document.getElementById('notifications').checked,
      syncIntervalSecs: syncInterval,
      explorerNavPane: document.getElementById('explorer-nav-pane').checked,
      cacheDir: document.getElementById('cache-dir').value || null,
      cacheMaxSize: document.getElementById('cache-max-size').value,
      metadataTtlSecs: metadataTtl,
      logLevel: document.getElementById('log-level').value,
    });
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}
```

- [ ] **Step 4: Add auto-save event listeners with debounce**

Replace the old `checkDirty` listeners with auto-save listeners:

```js
// Auto-save: immediate for toggles/selects, debounced for text inputs
let _saveTimer = null;
function debouncedSave() {
  clearTimeout(_saveTimer);
  _saveTimer = setTimeout(saveSettings, 500);
}

['auto-start', 'notifications', 'explorer-nav-pane', 'sync-interval', 'log-level'].forEach(id =>
  document.getElementById(id).addEventListener('change', saveSettings));
['cache-dir', 'cache-max-size', 'metadata-ttl'].forEach(id =>
  document.getElementById(id).addEventListener('input', debouncedSave));
```

- [ ] **Step 5: Update `clearCache()` to match new DOM**

The `btn-clear-cache` is now a small inline text button. Update the function to handle the new element:

```js
async function clearCache() {
  const btn = document.getElementById('btn-clear-cache');
  const origText = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Clearing…';
  try {
    await invoke('clear_cache');
    btn.disabled = false;
    btn.textContent = origText;
  } catch (e) {
    btn.disabled = false;
    btn.textContent = origText;
    showStatus('Failed to clear cache: ' + formatError(e), 'error');
  }
}
```

- [ ] **Step 6: Remove old event listeners, add new ones**

Remove: `document.getElementById('save-settings').addEventListener(...)` line.

The `btn-clear-cache`, `btn-add-mount`, `sign-out-btn`, `btn-redetect` listeners stay but ensure IDs match the new HTML.

- [ ] **Step 7: Update `renderHandlerList()` for new Override UX**

Replace the always-visible input+Set with a click-to-expand pattern:

```js
function renderHandlerList(handlers) {
  const list = document.getElementById('handler-list');
  list.innerHTML = '';

  handlers.forEach(h => {
    const li = document.createElement('li');
    li.className = 'setting-row';

    const info = document.createElement('div');
    info.className = 'setting-label';
    info.style.display = 'flex';
    info.style.alignItems = 'center';
    info.style.gap = '10px';

    const extEl = document.createElement('span');
    extEl.className = 'handler-ext';
    extEl.textContent = h.extension;
    info.appendChild(extEl);

    const nameEl = document.createElement('span');
    nameEl.className = 'handler-name';
    nameEl.textContent = h.handler_name || 'None';
    info.appendChild(nameEl);
    // Note: SOURCE_LABELS and the .badge element are intentionally removed per spec
    // (compact design shows only extension + handler name)

    const actions = document.createElement('div');
    actions.className = 'setting-control';

    const overrideBtn = document.createElement('button');
    overrideBtn.className = 'btn-ghost btn-sm';
    overrideBtn.textContent = h.source === 'override' ? 'Change' : 'Override';

    overrideBtn.addEventListener('click', () => {
      // Replace button with inline input + Set + Clear
      actions.innerHTML = '';
      const input = document.createElement('input');
      input.type = 'text';
      input.className = 'handler-override-input';
      input.placeholder = 'Handler ID';
      input.value = h.source === 'override' ? h.handler_id : '';

      const setBtn = document.createElement('button');
      setBtn.className = 'btn-ghost btn-sm';
      setBtn.textContent = 'Set';
      setBtn.addEventListener('click', () => saveHandlerOverride(h.extension, input.value));

      actions.appendChild(input);
      actions.appendChild(setBtn);

      if (h.source === 'override') {
        const clearBtn = document.createElement('button');
        clearBtn.className = 'btn-link btn-sm';
        clearBtn.textContent = 'Clear';
        clearBtn.addEventListener('click', () => clearHandlerOverride(h.extension));
        actions.appendChild(clearBtn);
      }

      input.focus();
    });

    actions.appendChild(overrideBtn);
    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });

  if (handlers.length === 0) {
    const empty = document.createElement('li');
    empty.className = 'handler-empty';
    empty.textContent = 'No file handlers found';
    list.appendChild(empty);
  }
}
```

- [ ] **Step 8: Update `loadMounts()` to use `.setting-row` class**

Change `li.className = 'mount-item' + (m.enabled ? '' : ' mount-disabled')` → `li.className = 'setting-row' + (m.enabled ? '' : ' mount-disabled')`.

The toggle label wrapper stays the same (`.toggle-switch` wrapping the checkbox + `.toggle-track`). The remove button keeps `.btn-icon btn-icon-danger` class. Ensure the SVG icons in remove buttons use `width="14" height="14"` (compact).

- [ ] **Step 9: Test the settings page end-to-end**

Run: `make run` (or Tauri dev mode)
- Open settings from tray
- Verify all 3 panels load (General, Mounts, About)
- Verify sidebar navigation (click + keyboard arrows)
- Toggle a setting → verify auto-save (no Save button, no feedback on success)
- Change text input → verify debounced save
- Toggle a mount → verify it works
- Click Override on a handler → verify inline input appears
- Click Clear Cache → verify it works

- [ ] **Step 10: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.js
git commit -m "feat(ui): update settings.js — auto-save, remove dirty tracking, new handler UX"
```

---

### Task 4: Rewrite Wizard HTML

**Files:**
- Rewrite: `crates/carminedesktop-app/dist/wizard.html`

- [ ] **Step 1: Write the wizard HTML with sidebar stepper layout**

Replace the entire `wizard.html`. Key structure:
- Same `.app-layout` as settings
- Sidebar: same logo markup as settings (`<div class="sidebar-header"><div class="sidebar-logo">C</div><span class="sidebar-title">Carmine</span></div>`), then `<div class="stepper-label">Setup</div>`, then `<div class="stepper-nav">` with 4 `.stepper-item` divs
- Main content: 4 `.step` divs
- Steps 1, 2, 4: add class `step-centered` (vertically centered content)
- Step 3: add class `step-scroll` (scrollable, top-aligned)
- Stepper items are NOT interactive (no `tabindex`, no click handlers, no `role="button"`) — purely visual progress indicators

Preserve all existing element IDs that JS references:
- `step-welcome`, `step-signing-in`, `step-sources`, `step-success`
- `sign-in-btn`, `copy-btn`, `cancel-btn`, `auth-url`, `auth-countdown`, `auth-error`
- `sources-loading`, `sources-onedrive-section`, `onedrive-check`, `onedrive-drive-name`, `onedrive-mount-path`, `onedrive-card`
- `sources-sp-section`, `sources-sp-search`, `sources-sp-spinner`, `sources-sp-error`, `sources-sp-sites`
- `sources-sp-libraries`, `sources-sp-back-sites`, `sources-sp-lib-list`, `add-selected-btn`
- `sources-added-section`, `sources-added-list`
- `sources-error`, `get-started-btn`, `switch-account-btn`
- `done-mount-list`, `wizard-close-btn`
- `status-bar`

Add stepper elements with IDs: `stepper-1`, `stepper-2`, `stepper-3`, `stepper-4` for JS to update.
Add footer element: `wizard-footer` for contextual content.

- [ ] **Step 2: Verify all element IDs referenced in wizard.js exist in the new HTML**

Cross-reference every `getElementById` and `querySelector` call.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/wizard.html
git commit -m "feat(ui): rewrite wizard.html — sidebar stepper layout, flat rows"
```

---

### Task 5: Update Wizard JS (selector fixes + stepper)

**Files:**
- Modify: `crates/carminedesktop-app/dist/wizard.js`

- [ ] **Step 1: Add stepper update function**

Add a function to update the sidebar stepper state:

```js
const STEP_MAP = {
  'step-welcome': 1,
  'step-signing-in': 2,
  'step-sources': 3,
  'step-success': 4,
};

function updateStepper(stepId) {
  const currentStep = STEP_MAP[stepId] || 1;
  for (let i = 1; i <= 4; i++) {
    const el = document.getElementById('stepper-' + i);
    if (!el) continue;
    el.classList.remove('active', 'done');
    if (i < currentStep) el.classList.add('done');
    else if (i === currentStep) el.classList.add('active');
  }
  // Update footer
  const footer = document.getElementById('wizard-footer');
  if (footer) {
    if (currentStep === 3) {
      const count = document.getElementById('sources-added-list').children.length;
      footer.textContent = count > 0 ? count + ' sources added' : '';
      footer.style.display = count > 0 ? '' : 'none';
    } else {
      footer.textContent = '';
      footer.style.display = 'none';
    }
  }
}
```

- [ ] **Step 2: Update `showStep()` to call `updateStepper()`**

```js
function showStep(id) {
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  document.getElementById(id).classList.add('active');
  if (_stepTitles[id]) document.title = _stepTitles[id];
  updateStepper(id);
}
```

- [ ] **Step 3: Update `addSourceEntry()` to use `.btn-icon` instead of `.btn-remove`**

Change `removeBtn.className = 'btn-remove'` to `removeBtn.className = 'btn-icon btn-icon-danger'` and use a trash SVG icon instead of text "Remove":

```js
removeBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>';
```

Also update the loading/disabled state: since the button is now an icon (not text), change `removeBtn.textContent = 'Removing…'` to `removeBtn.disabled = true` (no text change needed — the icon stays, just disabled opacity). On failure, set `removeBtn.disabled = false`.

Keep the row class `added-source-row` (it's in the new CSS).

- [ ] **Step 4: Update `renderFollowedSites()` to use new row classes**

Change `row.className = 'sp-result-row'` — this class is kept in the new CSS. No changes needed if the class name stays.

- [ ] **Step 5: Update library row rendering to use `.lib-row` instead of `.sp-lib-row`**

In `selectSiteInSources()`, change:
- `row.className = 'sp-lib-row'` → `row.className = 'lib-row'`

Also update ALL `.sp-lib-row` selectors across the entire file, including in `confirmSelectedLibraries()`:
- `'.sp-lib-row[data-drive-id="' + CSS.escape(driveId) + '"]'` → `'.lib-row[data-drive-id="' + CSS.escape(driveId) + '"]'`
(This selector appears twice in `confirmSelectedLibraries()` — update both occurrences.)

- [ ] **Step 6: Update `updateGetStartedBtn()` footer counter**

After updating the button state, also update the wizard footer source counter:

```js
function updateGetStartedBtn() {
  // ... existing logic ...
  // Update stepper footer
  const footer = document.getElementById('wizard-footer');
  if (footer) {
    const count = document.getElementById('sources-added-list').children.length;
    footer.textContent = count > 0 ? count + ' sources added' : '';
    footer.style.display = count > 0 ? '' : 'none';
  }
}
```

- [ ] **Step 7: Test the wizard end-to-end**

Run: `make run`
- Open wizard
- Verify stepper shows step 1 active, 2-4 muted
- Click Sign In → verify stepper updates to step 2
- Complete auth → verify stepper shows step 1 done, step 3 active
- Select sources → verify footer shows count
- Complete → verify step 4 shows all done + summary
- Verify "Sign in with a different account" link works

- [ ] **Step 8: Commit**

```bash
git add crates/carminedesktop-app/dist/wizard.js
git commit -m "feat(ui): update wizard.js — stepper state management, compact row classes"
```

---

## Chunk 2: Phase 2 — JS Refactoring

### Task 6: Refactor settings.js — Centralized State

**Files:**
- Rewrite: `crates/carminedesktop-app/dist/settings.js`

- [ ] **Step 1: Define the state object and `setState()` wrapper**

```js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const state = {
  settings: {},
  mounts: [],
  handlers: [],
  activePanel: 'general',
};

function setState(patch) {
  Object.assign(state, patch);
  render();
}
```

- [ ] **Step 2: Write `renderNav()` — sidebar navigation from state**

```js
function renderNav() {
  document.querySelectorAll('.nav-item').forEach(item => {
    const panel = item.dataset.panel;
    item.classList.toggle('active', panel === state.activePanel);
    item.setAttribute('aria-selected', panel === state.activePanel ? 'true' : 'false');
    item.setAttribute('tabindex', panel === state.activePanel ? '0' : '-1');
  });
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  const panel = document.getElementById('panel-' + state.activePanel);
  if (panel) panel.classList.add('active');
}
```

- [ ] **Step 3: Write `renderSettings()` — populate form fields from `state.settings`**

```js
function renderSettings() {
  const s = state.settings;
  if (!s || s.auto_start === undefined) return;
  document.getElementById('auto-start').checked = s.auto_start;
  document.getElementById('notifications').checked = s.notifications;
  document.getElementById('sync-interval').value = String(s.sync_interval_secs);
  document.getElementById('cache-dir').value = s.cache_dir || '';
  document.getElementById('cache-max-size').value = s.cache_max_size;
  document.getElementById('metadata-ttl').value = String(s.metadata_ttl_secs);
  document.getElementById('log-level').value = s.log_level;
  document.getElementById('account-email').textContent = s.account_display || 'Not signed in';
  const navPaneField = document.getElementById('nav-pane-field');
  if (navPaneField && s.platform === 'windows') {
    navPaneField.style.display = '';
    document.getElementById('explorer-nav-pane').checked = s.explorer_nav_pane;
  }
}
```

- [ ] **Step 4: Write `renderMounts()` — build mount rows from `state.mounts`**

Build mount list from `state.mounts`. Each mount row uses `data-action` and `data-id` attributes for delegation:

```js
function renderMounts() {
  const list = document.getElementById('mount-list');
  list.innerHTML = '';
  state.mounts.forEach(m => {
    const li = document.createElement('li');
    li.className = 'setting-row' + (m.enabled ? '' : ' mount-disabled');

    const info = document.createElement('div');
    info.className = 'mount-info';
    info.innerHTML = '<div class="mount-name">' + m.name + '</div>'
      + '<div class="mount-path">' + m.mount_point + '</div>';

    const actions = document.createElement('div');
    actions.className = 'mount-actions';

    const toggleLabel = document.createElement('label');
    toggleLabel.className = 'toggle-switch';
    const toggleInput = document.createElement('input');
    toggleInput.type = 'checkbox';
    toggleInput.checked = m.enabled;
    toggleInput.dataset.action = 'toggle-mount';
    toggleInput.dataset.id = m.id;
    const toggleTrack = document.createElement('span');
    toggleTrack.className = 'toggle-track';
    toggleLabel.appendChild(toggleInput);
    toggleLabel.appendChild(toggleTrack);

    const removeBtn = document.createElement('button');
    removeBtn.className = 'btn-icon btn-icon-danger';
    removeBtn.dataset.action = 'remove-mount';
    removeBtn.dataset.id = m.id;
    removeBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>';

    actions.appendChild(toggleLabel);
    actions.appendChild(removeBtn);
    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });
  if (state.mounts.length === 0) {
    const empty = document.createElement('li');
    empty.className = 'mount-empty';
    empty.textContent = 'No mounts configured';
    list.appendChild(empty);
  }
}
```

- [ ] **Step 5: Write `renderHandlers()` — build handler rows from `state.handlers`**

Same click-to-expand pattern as Task 3 Step 7, but reads from `state.handlers`. Use `data-action` attributes for the Override/Set/Clear buttons. The override expanded/collapsed state is handled ephemerally via DOM manipulation (not tracked in `state` — it resets on re-render, which is acceptable since re-renders only happen on data changes).

```js
function renderHandlers() {
  const list = document.getElementById('handler-list');
  list.innerHTML = '';
  state.handlers.forEach(h => {
    const li = document.createElement('li');
    li.className = 'setting-row';
    // Same DOM construction as Task 3 Step 7's renderHandlerList()
    // but using data-action="override-handler" data-ext="${h.extension}" on the Override button
    // and data-action="set-override" / data-action="clear-override" on the Set/Clear buttons
    // ... (same logic as Task 3 Step 7)
  });
}
```

The delegation handler in Step 7 handles these actions by finding the parent row and operating on it.

- [ ] **Step 6: Write `render()` coordinator**

```js
function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderHandlers();
}
```

- [ ] **Step 7: Set up event delegation on `.main-content` for dynamic elements**

Delegation handles only dynamically-rendered elements (mounts, handlers). Static buttons (`btn-redetect`, `btn-add-mount`, `btn-clear-cache`) keep direct `addEventListener` calls in `init()`.

```js
// Delegation for dynamically rendered mount and handler rows
document.querySelector('.main-content').addEventListener('click', async (e) => {
  const target = e.target.closest('[data-action]');
  if (!target) return;
  const action = target.dataset.action;

  if (action === 'toggle-mount') await toggleMount(target.dataset.id);
  else if (action === 'remove-mount') await removeMount(target.dataset.id);
  else if (action === 'override-handler') showOverrideInput(target);
  else if (action === 'set-override') await setOverride(target);
  else if (action === 'clear-override') await clearOverride(target);
});

// Also listen for 'change' events for toggle inputs (checkboxes fire 'change', not 'click')
document.querySelector('.main-content').addEventListener('change', async (e) => {
  const target = e.target.closest('[data-action]');
  if (!target) return;
  if (target.dataset.action === 'toggle-mount') await toggleMount(target.dataset.id);
});
```

- [ ] **Step 8: Write `init()` — single entry point**

```js
async function init() {
  try {
    const [settings, mounts, handlers] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
      invoke('get_file_handlers'),
    ]);
    setState({ settings, mounts, handlers });
    document.title = settings.app_name + ' Settings';
  } catch (e) {
    showStatus(formatError(e), 'error');
  }

  // Nav click + keyboard
  const navItems = Array.from(document.querySelectorAll('.nav-item'));
  function handleNavKeydown(e) {
    const idx = navItems.indexOf(e.currentTarget);
    let target = null;
    if (e.key === 'ArrowDown') target = navItems[(idx + 1) % navItems.length];
    else if (e.key === 'ArrowUp') target = navItems[(idx - 1 + navItems.length) % navItems.length];
    else if (e.key === 'Home') target = navItems[0];
    else if (e.key === 'End') target = navItems[navItems.length - 1];
    else if (e.key === 'Enter' || e.key === ' ') target = e.currentTarget;
    if (target) { e.preventDefault(); setState({ activePanel: target.dataset.panel }); target.focus(); }
  }
  navItems.forEach(item => {
    item.addEventListener('click', () => setState({ activePanel: item.dataset.panel }));
    item.addEventListener('keydown', handleNavKeydown);
  });

  // Auto-save listeners
  ['auto-start', 'notifications', 'explorer-nav-pane', 'sync-interval', 'log-level'].forEach(id =>
    document.getElementById(id).addEventListener('change', saveSettings));
  ['cache-dir', 'cache-max-size', 'metadata-ttl'].forEach(id =>
    document.getElementById(id).addEventListener('input', debouncedSave));

  // Static buttons (direct listeners — not delegated)
  document.getElementById('sign-out-btn').addEventListener('click', signOut);
  document.getElementById('btn-add-mount').addEventListener('click', addMount);
  document.getElementById('btn-clear-cache').addEventListener('click', clearCache);
  document.getElementById('btn-redetect').addEventListener('click', redetectHandlers);

  // Backend-triggered refresh
  listen('refresh-settings', async () => {
    const [settings, mounts] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
    ]);
    setState({ settings, mounts });
  });
}

init();
```

- [ ] **Step 9: Remove all old top-level code**

Delete the old standalone calls: `loadSettings()`, `loadMounts()`, `loadFileHandlers()`, all old `addEventListener` registrations at the bottom.

- [ ] **Step 10: Test all settings functionality**

Same test plan as Task 3 Step 9. Additionally verify:
- State is consistent after `refresh-settings` event
- Handler override expand/collapse works via delegation
- Mount toggle/remove works via delegation

- [ ] **Step 11: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.js
git commit -m "refactor(ui): settings.js — centralized state, render functions, event delegation"
```

---

### Task 7: Refactor wizard.js — Centralized State

**Files:**
- Rewrite: `crates/carminedesktop-app/dist/wizard.js`

- [ ] **Step 1: Define the state object**

```js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const state = {
  step: 'step-welcome',
  signingIn: false,
  addMountMode: false,
  authUrl: '',
  onedriveDriveId: null,
  defaultMountRoot: '~/Cloud',
  followedSites: [],
  selectedSite: null,
  libraries: [],
  selectedLibraries: new Map(), // driveId → { site: {id, display_name, web_url}, library: {id, name} }
  addedSources: [],             // { label, mountId }
  authUnlisteners: [],          // unlisten functions from Tauri listen()
  finalMounts: [],
};
```

- [ ] **Step 2: Write `goToStep(stepId)` — centralized navigation**

```js
function goToStep(stepId) {
  state.step = stepId;
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  document.getElementById(stepId).classList.add('active');
  updateStepper(stepId);
  if (_stepTitles[stepId]) document.title = _stepTitles[stepId];
}
```

- [ ] **Step 3: Write `renderStepper()` — reads from `state.step`**

Same logic as Task 5's `updateStepper()` but reads from `state` directly.

- [ ] **Step 4: Write `renderSources()` — builds source list from state**

This is the most complex render function. It handles 4 sub-sections:

```js
function renderSources() {
  // 1. OneDrive section
  const odSection = document.getElementById('sources-onedrive-section');
  if (state.onedriveDriveId && !state.addMountMode) {
    odSection.style.display = 'block';
    document.getElementById('onedrive-drive-name').textContent = 'OneDrive';
    document.getElementById('onedrive-mount-path').textContent = state.defaultMountRoot + '/OneDrive';
  } else {
    odSection.style.display = 'none';
  }

  // 2. Followed sites / search results
  const sitesEl = document.getElementById('sources-sp-sites');
  sitesEl.innerHTML = '';
  state.followedSites.forEach(site => {
    const row = document.createElement('div');
    row.className = 'sp-result-row';
    row.dataset.action = 'select-site';
    row.dataset.siteId = site.id;
    row.setAttribute('role', 'button');
    row.setAttribute('tabindex', '0');
    row.innerHTML = '<div>' + site.display_name + '</div><div class="sp-result-url">' + site.web_url + '</div>';
    sitesEl.appendChild(row);
  });

  // 3. Libraries (if a site is selected)
  if (state.selectedSite) {
    document.getElementById('sources-sp-libraries').style.display = 'block';
    const libList = document.getElementById('sources-sp-lib-list');
    libList.innerHTML = '';
    // ... render library rows with data-action="toggle-lib" data-drive-id="${lib.id}"
    // Same checkbox/selected/mounted logic as current selectSiteInSources()
  }

  // 4. Added sources
  const addedSection = document.getElementById('sources-added-section');
  const addedList = document.getElementById('sources-added-list');
  addedList.innerHTML = '';
  state.addedSources.forEach(s => {
    const row = document.createElement('div');
    row.className = 'added-source-row';
    row.innerHTML = '<div class="added-source-name">' + s.label + '</div>';
    const btn = document.createElement('button');
    btn.className = 'btn-icon btn-icon-danger';
    btn.dataset.action = 'remove-source';
    btn.dataset.mountId = s.mountId;
    btn.innerHTML = '/* trash SVG */';
    row.appendChild(btn);
    addedList.appendChild(row);
  });
  addedSection.style.display = state.addedSources.length > 0 ? 'block' : 'none';
}
```

- [ ] **Step 5: Migrate functions to use `state` — by function**

Update each function individually. Key mappings:

| Old global | New state path | Functions affected |
|-----------|---------------|-------------------|
| `signingIn` | `state.signingIn` | `startSignIn()`, `cancelSignIn()`, auth listeners |
| `onedriveDriveId` | `state.onedriveDriveId` | `loadSources()`, `getStarted()` |
| `sourcesSelectedSite` | `state.selectedSite` | `selectSiteInSources()` |
| `selectedLibraries` | `state.selectedLibraries` | `selectSiteInSources()`, `confirmSelectedLibraries()`, `updateAddSelectedBtn()` |
| `cachedFollowedSites` | `state.followedSites` | `loadSources()`, `onSourcesSpSearchInput()` |
| `addMountMode` | `state.addMountMode` | `goToAddMount()`, `loadSources()`, `getStarted()`, `switchAccount()` |
| `defaultMountRoot` | `state.defaultMountRoot` | `init()`, `loadSources()`, `getStarted()`, `confirmSelectedLibraries()` |
| `countdownTimer` | local variable in `startCountdown()` | `startCountdown()`, `stopCountdown()` — keep countdown as direct DOM manipulation (interval + DOM update), not state-driven |

- [ ] **Step 6: Write `init()` — single entry point with delegation**

```js
async function init() {
  try {
    state.defaultMountRoot = await invoke('get_default_mount_root');
    state.defaultMountRoot = state.defaultMountRoot.replace(/[/\\]+$/, '');
  } catch (e) {
    console.warn('get_default_mount_root failed, using fallback:', e);
  }

  // Static button listeners
  document.getElementById('sign-in-btn').addEventListener('click', startSignIn);
  document.getElementById('copy-btn').addEventListener('click', copyAuthUrl);
  document.getElementById('cancel-btn').addEventListener('click', cancelSignIn);
  document.getElementById('sources-sp-back-sites').addEventListener('click', backToSites);
  document.getElementById('add-selected-btn').addEventListener('click', confirmSelectedLibraries);
  document.getElementById('sources-sp-search').addEventListener('input', onSourcesSpSearchInput);
  document.getElementById('onedrive-check').addEventListener('change', updateGetStartedBtn);
  document.getElementById('get-started-btn').addEventListener('click', handleGetStarted);
  document.getElementById('wizard-close-btn').addEventListener('click', () => {
    window.__TAURI__.window.getCurrentWindow().close();
  });
  document.getElementById('switch-account-btn').addEventListener('click', switchAccount);

  // Delegation for dynamic rows (sites, libraries, added sources)
  document.querySelector('.main-content').addEventListener('click', (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    if (target.dataset.action === 'select-site') selectSiteById(target.dataset.siteId);
    else if (target.dataset.action === 'toggle-lib') toggleLibrary(target.dataset.driveId);
    else if (target.dataset.action === 'remove-source') removeSource(target.dataset.mountId);
  });
  // Keyboard support for site rows
  document.querySelector('.main-content').addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    const target = e.target.closest('[data-action]');
    if (target) { e.preventDefault(); target.click(); }
  });

  listen('navigate-add-mount', () => goToAddMount());

  const authenticated = await invoke('is_authenticated').catch(() => false);
  if (authenticated) await goToAddMount();
}
init();
```

- [ ] **Step 7: Replace auth listener cleanup with unlisten array on state**

Tauri's `listen()` returns an unlisten function (not compatible with `AbortController`). Store unlisteners on `state`:

```js
// state already has: authUnlisteners: []

async function startSignIn() {
  state.signingIn = true;
  // ... (FUSE check, show step, start countdown)

  state.authUnlisteners.push(await listen('auth-complete', async () => {
    if (!state.signingIn) return;
    state.signingIn = false;
    stopCountdown();
    cleanupAuthListeners();
    await onSignInComplete();
  }));
  state.authUnlisteners.push(await listen('auth-error', (event) => {
    if (!state.signingIn) return;
    state.signingIn = false;
    stopCountdown();
    cleanupAuthListeners();
    // ... show error
  }));
  // ... invoke start_sign_in
}

function cleanupAuthListeners() {
  state.authUnlisteners.forEach(fn => { try { fn(); } catch (_) {} });
  state.authUnlisteners = [];
}
```

Add `authUnlisteners: []` to the state object in Step 1.

- [ ] **Step 8: Test the wizard end-to-end**

Same test plan as Task 5 Step 7. Additionally verify:
- No console errors about undefined globals
- State is consistent after switching accounts
- `navigate-add-mount` event works

- [ ] **Step 9: Commit**

```bash
git add crates/carminedesktop-app/dist/wizard.js
git commit -m "refactor(ui): wizard.js — centralized state, remove globals, declarative rendering"
```

---

### Task 8: Final Cleanup

**Files:**
- Modify: `crates/carminedesktop-app/dist/styles.css` (if any orphan CSS remains)

- [ ] **Step 1: Search for orphan CSS classes**

Grep all CSS class names in `styles.css` and verify each one is referenced in at least one HTML or JS file.

- [ ] **Step 2: Remove any unused classes**

Delete CSS rules for classes that are no longer referenced.

- [ ] **Step 3: Full integration test**

1. `make build` — verify no Rust warnings
2. Run the app on host
3. Test wizard flow: Welcome → Sign In → Add Sources → Done
4. Test settings: all 3 panels, auto-save, mount toggle/remove, handler override, clear cache
5. Test keyboard navigation in sidebar
6. Test tray menu → Settings, tray menu → Add Mount (opens wizard in add-mount mode)
7. Verify no console errors about undefined variables or missing elements
8. Verify all `data-action` attributes in HTML/JS-rendered DOM match delegation handlers

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/dist/
git commit -m "chore(ui): cleanup orphan CSS after UI/UX refonte"
```
