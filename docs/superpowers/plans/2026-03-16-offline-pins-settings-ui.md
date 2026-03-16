# Offline Pins Settings UI — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an "Offline" tab to the settings page that lists all pinned offline directories with per-pin toggle (remove/active) and auto-removal timing info, plus TTL and max folder size settings.

**Architecture:** New `list_offline_pins` and `remove_offline_pin` Tauri commands read from PinStore + SqliteStore to resolve item names. Frontend adds a 4th "Offline" nav tab with a dynamic pin list (folder name, mount name, time remaining, toggle) and two offline config fields (TTL selector, max folder size input). Follows existing settings.js patterns: state + render + event delegation.

**Tech Stack:** Rust (Tauri commands, serde), Vanilla JS, HTML, CSS

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/carminedesktop-cache/src/pin_store.rs` | Modify | Add `Serialize` derive to `PinnedFolder` |
| `crates/carminedesktop-app/src/commands.rs` | Modify | Add `OfflinePinInfo` struct, `list_offline_pins` and `remove_offline_pin` commands |
| `crates/carminedesktop-app/src/main.rs` | Modify | Register new commands in `invoke_handler!` |
| `crates/carminedesktop-app/dist/settings.html` | Modify | Add "Offline" nav tab + `#panel-offline` with pin list and settings |
| `crates/carminedesktop-app/dist/settings.js` | Modify | Add `offlinePins` to state, `renderOfflinePins()`, `removeOfflinePin()`, wire offline settings |
| `crates/carminedesktop-app/dist/styles.css` | Modify | Add `.pin-row`, `.pin-info`, `.pin-expiry` styles |

---

## Chunk 1: Backend — Tauri Commands

### Task 1: Make PinnedFolder serializable

**Files:**
- Modify: `crates/carminedesktop-cache/src/pin_store.rs:5-12`

The frontend needs pin data. `PinnedFolder` must derive `Serialize` + `Clone` (already has Clone).

- [ ] **Step 1: Add serde derive to PinnedFolder**

In `crates/carminedesktop-cache/src/pin_store.rs`, add `serde::Serialize` derive:

```rust
use serde::Serialize;

/// A single pinned folder record.
#[derive(Debug, Clone, Serialize)]
pub struct PinnedFolder {
    pub drive_id: String,
    pub item_id: String,
    pub pinned_at: String,
    pub expires_at: String,
}
```

Note: `serde` is already a workspace dependency with `derive` feature (used throughout the project).

- [ ] **Step 2: Verify it compiles**

Run: `make check`
Expected: PASS — no errors

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-cache/src/pin_store.rs
git commit -m "feat(cache): derive Serialize on PinnedFolder for frontend use"
```

### Task 2: Add list_offline_pins command

**Files:**
- Modify: `crates/carminedesktop-app/src/commands.rs`

This command iterates all mount caches, lists pins from each PinStore, and resolves item_id → folder name via SqliteStore metadata cache.

- [ ] **Step 1: Add OfflinePinInfo struct**

In `commands.rs`, after the `DriveInfo` struct (line 55), add:

```rust
#[derive(Serialize)]
pub struct OfflinePinInfo {
    pub drive_id: String,
    pub item_id: String,
    pub folder_name: String,
    pub mount_name: String,
    pub pinned_at: String,
    pub expires_at: String,
}
```

- [ ] **Step 2: Add list_offline_pins command**

After the `get_settings` function (around line 430), add:

```rust
#[tauri::command]
pub fn list_offline_pins(app: AppHandle) -> Result<Vec<OfflinePinInfo>, String> {
    let state = app.state::<AppState>();

    // Collect Arc refs and mount names under the lock, then drop it.
    let entries: Vec<(String, String, Arc<CacheManager>)> = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        caches
            .iter()
            .map(|(drive_id, (cache, _, _, _, _))| {
                let mount_name = config
                    .mounts
                    .iter()
                    .find(|m| m.drive_id.as_deref() == Some(drive_id))
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| drive_id.clone());
                (drive_id.clone(), mount_name, cache.clone())
            })
            .collect()
    };

    let mut pins = Vec::new();
    for (drive_id, mount_name, cache) in &entries {
        let all_pins = cache.pin_store.list_all().map_err(|e| e.to_string())?;

        for pin in all_pins {
            let folder_name = cache
                .sqlite
                .get_item_by_id(&pin.item_id)
                .ok()
                .flatten()
                .map(|(_, item)| item.name)
                .unwrap_or_else(|| pin.item_id.clone());

            pins.push(OfflinePinInfo {
                drive_id: pin.drive_id,
                item_id: pin.item_id,
                folder_name,
                mount_name: mount_name.clone(),
                pinned_at: pin.pinned_at,
                expires_at: pin.expires_at,
            });
        }
    }

    Ok(pins)
}
```

- [ ] **Step 3: Verify it compiles**

Run: `make check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/src/commands.rs
git commit -m "feat(app): add list_offline_pins command"
```

### Task 3: Add remove_offline_pin command

**Files:**
- Modify: `crates/carminedesktop-app/src/commands.rs`

- [ ] **Step 1: Add remove_offline_pin command**

After `list_offline_pins`, add:

```rust
#[tauri::command]
pub fn remove_offline_pin(
    app: AppHandle,
    drive_id: String,
    item_id: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Clone Arc out of the lock, then drop it.
    let offline_mgr = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (_, _, _, mgr, _) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no mount found for drive {drive_id}"))?;
        mgr.clone()
    };

    offline_mgr
        .unpin_folder(&item_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `make check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/src/commands.rs
git commit -m "feat(app): add remove_offline_pin command"
```

### Task 4: Register new commands in invoke_handler

**Files:**
- Modify: `crates/carminedesktop-app/src/main.rs:587-615`

- [ ] **Step 1: Add both commands to invoke_handler**

In the `invoke_handler!` macro call (line 587), add `commands::list_offline_pins` and `commands::remove_offline_pin` alongside the existing commands:

```rust
commands::clear_file_handler_override,
commands::list_offline_pins,
commands::remove_offline_pin,
```

- [ ] **Step 2: Run clippy to verify**

Run: `make clippy`
Expected: PASS — zero warnings

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/src/main.rs
git commit -m "feat(app): register offline pin commands in invoke_handler"
```

---

## Chunk 2: Frontend — HTML + CSS

### Task 5: Add Offline nav tab to settings.html

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.html:17-29`

- [ ] **Step 1: Add the Offline tab button**

After the "Mounts" nav-item button (line 25) and before the "About" button (line 26), insert a new button:

```html
<button class="nav-item" data-panel="offline" role="tab" tabindex="-1" aria-selected="false" aria-controls="panel-offline" id="tab-offline">
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
  Offline
</button>
```

Icon: download arrow — represents offline availability.

- [ ] **Step 2: Verify no inline handlers (CSP compliance)**

Confirm: no `onclick` or other inline event handlers. The button uses `data-panel` like existing tabs. PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.html
git commit -m "feat(ui): add Offline tab to settings navigation"
```

### Task 6: Add Offline panel HTML

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.html:159-160`

- [ ] **Step 1: Add the Offline panel**

After the Mounts panel closing `</div>` (line 159) and before the About panel (line 161), insert:

```html
<!-- Offline panel -->
<div class="panel" id="panel-offline" role="tabpanel" aria-labelledby="tab-offline">
  <p class="section-heading">Pinned Folders</p>
  <ul class="pin-list" id="pin-list"></ul>

  <p class="section-heading">Settings</p>

  <div class="setting-row">
    <div class="setting-label">
      <div class="label-text">Auto-removal</div>
      <div class="label-sub">How long pinned folders stay available offline</div>
    </div>
    <div class="setting-control">
      <select id="offline-ttl">
        <option value="3600">1 hour</option>
        <option value="86400">1 day</option>
        <option value="259200">3 days</option>
        <option value="604800">7 days</option>
      </select>
    </div>
  </div>

  <div class="setting-row">
    <div class="setting-label">
      <div class="label-text">Max folder size</div>
      <div class="label-sub">Largest folder allowed for offline pinning</div>
    </div>
    <div class="setting-control">
      <input type="text" id="offline-max-size" placeholder="5GB">
    </div>
  </div>
</div>
```

- [ ] **Step 2: Verify no inline handlers**

Confirmed: all controls use IDs, event listeners will be added in JS. PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.html
git commit -m "feat(ui): add Offline panel with pin list and settings fields"
```

### Task 7: Add pin-row CSS styles

**Files:**
- Modify: `crates/carminedesktop-app/dist/styles.css` (append after line 278, before `#status-bar`)

- [ ] **Step 1: Add pin list styles**

Before the `#status-bar` block (line 279), insert:

```css
/* ── Pin list (Offline panel) ─────────────────────────────── */
.pin-list { list-style: none; padding: 0; }
.pin-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 0;
  border-bottom: 1px solid var(--border-row);
}
.pin-row:last-child { border-bottom: none; }
.pin-info { flex: 1; min-width: 0; }
.pin-name { font-size: 13px; color: var(--text-primary); }
.pin-meta { font-size: 11px; color: var(--text-muted); margin-top: 1px; }
.pin-expiry { color: var(--text-secondary); }
.pin-expiry.expired { color: var(--danger); }
.pin-actions { display: flex; align-items: center; gap: 12px; flex-shrink: 0; }
.pin-empty { color: var(--text-muted); font-style: italic; padding: 9px 0; }
```

- [ ] **Step 2: Verify visual consistency**

The styles mirror `.mount-info`, `.mount-name`, `.mount-path`, `.mount-actions` patterns for consistency.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/styles.css
git commit -m "feat(ui): add pin-row styles for offline panel"
```

---

## Chunk 3: Frontend — JavaScript Logic

### Task 8: Add offlinePins to state and renderOfflinePins function

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.js`

- [ ] **Step 1: Add offlinePins to state**

In the state object (line 8-13), add `offlinePins`:

```javascript
const state = {
  settings: {},
  mounts: [],
  handlers: [],
  offlinePins: [],
  activePanel: 'general',
};
```

- [ ] **Step 2: Add renderOfflinePins function**

After `renderHandlers()` (line 152) and before `render()` (line 154), add:

```javascript
function formatTimeRemaining(expiresAt) {
  const now = new Date();
  const expires = new Date(expiresAt + 'Z');
  const diffMs = expires - now;
  if (diffMs <= 0) return { text: 'Expired', expired: true };
  const hours = Math.floor(diffMs / 3600000);
  const days = Math.floor(hours / 24);
  if (days > 0) return { text: days + 'd ' + (hours % 24) + 'h remaining', expired: false };
  const mins = Math.floor((diffMs % 3600000) / 60000);
  if (hours > 0) return { text: hours + 'h ' + mins + 'm remaining', expired: false };
  return { text: mins + 'm remaining', expired: false };
}

function renderOfflinePins() {
  const list = document.getElementById('pin-list');
  if (!list) return;
  list.innerHTML = '';

  state.offlinePins.forEach(pin => {
    const li = document.createElement('li');
    li.className = 'pin-row';

    const info = document.createElement('div');
    info.className = 'pin-info';
    const nameEl = document.createElement('div');
    nameEl.className = 'pin-name';
    nameEl.textContent = pin.folder_name;
    const metaEl = document.createElement('div');
    metaEl.className = 'pin-meta';
    const remaining = formatTimeRemaining(pin.expires_at);
    const expirySpan = document.createElement('span');
    expirySpan.className = 'pin-expiry' + (remaining.expired ? ' expired' : '');
    expirySpan.textContent = remaining.text;
    metaEl.appendChild(document.createTextNode(pin.mount_name + ' \u00B7 '));
    metaEl.appendChild(expirySpan);
    info.appendChild(nameEl);
    info.appendChild(metaEl);

    const actions = document.createElement('div');
    actions.className = 'pin-actions';
    const removeBtn = document.createElement('button');
    removeBtn.className = 'btn-icon btn-icon-danger';
    removeBtn.dataset.action = 'remove-pin';
    removeBtn.dataset.driveId = pin.drive_id;
    removeBtn.dataset.itemId = pin.item_id;
    removeBtn.dataset.name = pin.folder_name;
    removeBtn.title = 'Remove offline pin';
    removeBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>';

    actions.appendChild(removeBtn);
    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });

  if (state.offlinePins.length === 0) {
    const empty = document.createElement('li');
    empty.className = 'pin-empty';
    empty.textContent = 'No folders pinned for offline use';
    list.appendChild(empty);
  }
}
```

- [ ] **Step 3: Add renderOfflinePins to render()**

Update the `render()` function (line 154) to also call `renderOfflinePins()`:

```javascript
function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderHandlers();
  renderOfflinePins();
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.js
git commit -m "feat(ui): add offlinePins state and renderOfflinePins function"
```

### Task 9: Add renderOfflineSettings to populate TTL and max size

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.js`

- [ ] **Step 1: Extend renderSettings to populate offline fields**

In `renderSettings()` (line 36), after the last setting (around `navPaneField` block, line 51), add:

```javascript
  const offlineTtl = document.getElementById('offline-ttl');
  if (offlineTtl) offlineTtl.value = String(s.offline_ttl_secs);
  const offlineMaxSize = document.getElementById('offline-max-size');
  if (offlineMaxSize) offlineMaxSize.value = s.offline_max_folder_size;
```

- [ ] **Step 2: Extend saveSettings to include offline fields**

In `saveSettings()` (line 165), add the offline params to the `invoke` call. After `logLevel` (line 185):

```javascript
      offlineTtlSecs: parseInt(document.getElementById('offline-ttl').value) || null,
      offlineMaxFolderSize: document.getElementById('offline-max-size').value || null,
```

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.js
git commit -m "feat(ui): wire offline TTL and max size settings to save/load"
```

### Task 10: Add removeOfflinePin action and event wiring

**Files:**
- Modify: `crates/carminedesktop-app/dist/settings.js`

- [ ] **Step 1: Add removeOfflinePin function**

After `clearCache()` (line 267), add:

```javascript
async function removeOfflinePin(driveId, itemId, name) {
  try {
    await invoke('remove_offline_pin', { driveId, itemId });
    showStatus('Unpinned ' + name, 'success');
    const offlinePins = await invoke('list_offline_pins');
    setState({ offlinePins });
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}
```

- [ ] **Step 2: Wire click delegation for remove-pin action**

In the delegated click handler (line 409), add a new case inside the handler after the existing actions:

```javascript
    else if (action === 'remove-pin') await removeOfflinePin(target.dataset.driveId, target.dataset.itemId, target.dataset.name);
```

- [ ] **Step 3: Wire auto-save listeners for offline fields**

In the `init()` function, in the auto-save listeners section (around line 397), add `'offline-ttl'` to the change listeners and `'offline-max-size'` to the input (debounced) listeners:

```javascript
  ['auto-start', 'notifications', 'explorer-nav-pane', 'sync-interval', 'log-level', 'offline-ttl'].forEach(id =>
    document.getElementById(id).addEventListener('change', saveSettings));
  ['cache-dir', 'cache-max-size', 'metadata-ttl', 'offline-max-size'].forEach(id =>
    document.getElementById(id).addEventListener('input', debouncedSave));
```

- [ ] **Step 4: Load offlinePins on init**

In `init()`, extend the `Promise.all` (line 368) to also load pins:

```javascript
    const [settings, mounts, handlers, offlinePins] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
      invoke('get_file_handlers'),
      invoke('list_offline_pins'),
    ]);
    setState({ settings, mounts, handlers, offlinePins });
```

- [ ] **Step 5: Refresh offlinePins on backend refresh event**

In the `refresh-settings` listener (line 426), also load pins:

```javascript
  listen('refresh-settings', async () => {
    const [settings, mounts, offlinePins] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
      invoke('list_offline_pins'),
    ]);
    setState({ settings, mounts, offlinePins });
  });
```

- [ ] **Step 6: Commit**

```bash
git add crates/carminedesktop-app/dist/settings.js
git commit -m "feat(ui): wire offline pin removal, auto-save, and init loading"
```

---

## Chunk 4: Verification and Polish

### Task 11: Full build and clippy check

**Files:** All modified files

- [ ] **Step 1: Run full build**

Run: `make build`
Expected: PASS — zero warnings (CI enforces `-Dwarnings`)

- [ ] **Step 2: Run clippy**

Run: `make clippy`
Expected: PASS — zero warnings

- [ ] **Step 3: Run tests**

Run: `make test`
Expected: PASS — all existing tests still pass

- [ ] **Step 4: Commit (if any fixups needed)**

```bash
git add -u
git commit -m "fix: address clippy/build warnings from offline pins UI"
```

### Task 12: Manual smoke test

- [ ] **Step 1: Start the app on host**

Launch Carmine Desktop. Open settings window.

- [ ] **Step 2: Verify Offline tab appears**

Click the "Offline" tab in the sidebar navigation. Confirm:
- Tab activates with red accent background
- Panel shows "Pinned Folders" section with empty state ("No folders pinned for offline use")
- Panel shows "Settings" section with TTL dropdown and max size input
- TTL dropdown shows the current config value (e.g., "1 day" for 86400)
- Max size shows the current config value (e.g., "5GB")

- [ ] **Step 3: Pin a folder via CLI and verify it appears**

Run: `carmine --offline-pin ~/Cloud/SomeFolder`

Go back to Settings → Offline tab. Verify:
- The folder appears in the pin list with correct name
- Mount name is shown (e.g., "OneDrive" or SharePoint library name)
- Time remaining is shown (e.g., "23h 59m remaining")
- Remove (trash) button is visible

- [ ] **Step 4: Remove a pin via UI**

Click the trash icon on the pin row. Verify:
- Status bar shows "Unpinned SomeFolder" success message
- The pin disappears from the list
- Empty state returns if no more pins

- [ ] **Step 5: Change offline settings**

Change TTL dropdown to "7 days". Change max size to "10GB". Verify:
- No error in status bar
- Settings are saved (reopen settings to confirm values persist)
