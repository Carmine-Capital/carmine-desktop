const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let _savedValues = {};

function snapshotValues() {
  _savedValues = {
    auto_start: document.getElementById('auto-start').checked,
    notifications: document.getElementById('notifications').checked,
    explorer_nav_pane: document.getElementById('explorer-nav-pane').checked,
    sync_interval: document.getElementById('sync-interval').value,
    cache_dir: document.getElementById('cache-dir').value,
    cache_max_size: document.getElementById('cache-max-size').value,
    metadata_ttl: document.getElementById('metadata-ttl').value,
    log_level: document.getElementById('log-level').value,
  };
}

function checkDirty() {
  const dirty =
    _savedValues.auto_start !== document.getElementById('auto-start').checked ||
    _savedValues.notifications !== document.getElementById('notifications').checked ||
    _savedValues.explorer_nav_pane !== document.getElementById('explorer-nav-pane').checked ||
    _savedValues.sync_interval !== document.getElementById('sync-interval').value ||
    _savedValues.cache_dir !== document.getElementById('cache-dir').value ||
    _savedValues.cache_max_size !== document.getElementById('cache-max-size').value ||
    _savedValues.metadata_ttl !== document.getElementById('metadata-ttl').value ||
    _savedValues.log_level !== document.getElementById('log-level').value;

  const badge = document.getElementById('unsaved-badge');
  if (badge) badge.style.display = dirty ? 'block' : 'none';
}

// Sidebar navigation
const navItems = Array.from(document.querySelectorAll('.nav-item'));

function activatePanel(navItem) {
  navItems.forEach(n => {
    n.classList.remove('active');
    n.setAttribute('aria-selected', 'false');
    n.setAttribute('tabindex', '-1');
  });
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  navItem.classList.add('active');
  navItem.setAttribute('aria-selected', 'true');
  navItem.setAttribute('tabindex', '0');
  const panelId = 'panel-' + navItem.dataset.panel;
  const panel = document.getElementById(panelId);
  if (panel) panel.classList.add('active');
  navItem.focus();
}

navItems.forEach(item => {
  item.addEventListener('click', () => activatePanel(item));
  item.addEventListener('keydown', (e) => {
    const idx = navItems.indexOf(item);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      activatePanel(navItems[(idx + 1) % navItems.length]);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      activatePanel(navItems[(idx - 1 + navItems.length) % navItems.length]);
    } else if (e.key === 'Home') {
      e.preventDefault();
      activatePanel(navItems[0]);
    } else if (e.key === 'End') {
      e.preventDefault();
      activatePanel(navItems[navItems.length - 1]);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activatePanel(item);
    }
  });
});

async function loadSettings() {
  try {
    const s = await invoke('get_settings');
    document.title = s.app_name + ' Settings';
    document.getElementById('auto-start').checked = s.auto_start;
    document.getElementById('notifications').checked = s.notifications;
    document.getElementById('sync-interval').value = String(s.sync_interval_secs);
    document.getElementById('cache-dir').value = s.cache_dir || '';
    document.getElementById('cache-max-size').value = s.cache_max_size;
    document.getElementById('metadata-ttl').value = String(s.metadata_ttl_secs);
    document.getElementById('log-level').value = s.log_level;
    // Populate account email in sidebar footer
    document.getElementById('account-email').textContent = s.account_display || 'Not signed in';
    // Show nav pane toggle only on Windows
    const navPaneField = document.getElementById('nav-pane-field');
    if (navPaneField && s.platform === 'windows') {
      navPaneField.style.display = '';
      document.getElementById('explorer-nav-pane').checked = s.explorer_nav_pane;
    }
    snapshotValues();
  } catch (e) {
    console.error(e);
    showStatus('Failed to load settings', 'error');
  }
}

async function loadMounts() {
  try {
    const mounts = await invoke('list_mounts');
    const list = document.getElementById('mount-list');
    list.innerHTML = '';
    mounts.forEach(m => {
      const li = document.createElement('li');
      li.className = 'mount-item' + (m.enabled ? '' : ' mount-disabled');

      const info = document.createElement('div');
      info.className = 'mount-info';
      const nameEl = document.createElement('div');
      nameEl.className = 'mount-name';
      nameEl.textContent = m.name;
      const pathEl = document.createElement('div');
      pathEl.className = 'mount-path';
      pathEl.textContent = m.mount_point;
      info.appendChild(nameEl);
      info.appendChild(pathEl);

      const actions = document.createElement('div');
      actions.className = 'mount-actions';

      const toggleLabel = document.createElement('label');
      toggleLabel.className = 'toggle-switch';
      toggleLabel.title = m.enabled ? 'Disable mount' : 'Enable mount';
      const toggleInput = document.createElement('input');
      toggleInput.type = 'checkbox';
      toggleInput.id = 'toggle-btn-' + m.id;
      toggleInput.checked = m.enabled;
      toggleInput.addEventListener('change', () => toggleMount(m.id));
      const toggleTrack = document.createElement('span');
      toggleTrack.className = 'toggle-track';
      toggleLabel.appendChild(toggleInput);
      toggleLabel.appendChild(toggleTrack);

      const removeBtn = document.createElement('button');
      removeBtn.id = 'remove-btn-' + m.id;
      removeBtn.className = 'btn-icon btn-icon-danger';
      removeBtn.title = 'Remove mount';
      removeBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>';
      removeBtn.addEventListener('click', () => removeMount(m.id));

      actions.appendChild(toggleLabel);
      actions.appendChild(removeBtn);

      li.appendChild(info);
      li.appendChild(actions);
      list.appendChild(li);
    });
    if (mounts.length === 0) {
      const empty = document.createElement('li');
      empty.className = 'mount-empty';
      empty.textContent = 'No mounts configured';
      list.appendChild(empty);
    }
  } catch (e) {
    console.error(e);
    showStatus('Failed to load mounts', 'error');
  }
}

// Combined save function for all settings (general + advanced)
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
  const btn = document.getElementById('save-settings');
  btn.disabled = true;
  btn.textContent = 'Saving…';
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
    btn.disabled = false;
    btn.textContent = 'Save';
    snapshotValues();
    checkDirty();
    showStatus('Settings saved', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Save';
    showStatus(formatError(e), 'error');
  }
}

async function toggleMount(id) {
  const toggle = document.getElementById('toggle-btn-' + id);
  toggle.disabled = true;
  try {
    await invoke('toggle_mount', { id });
    showStatus('Mount updated', 'success');
    loadMounts();
  } catch (e) {
    toggle.checked = !toggle.checked;
    toggle.disabled = false;
    showStatus(formatError(e), 'error');
  }
}

async function removeMount(id) {
  const ok = await window.__TAURI__.dialog.confirm('Remove this mount? This cannot be undone.', { title: 'Remove Mount', kind: 'warning' });
  if (!ok) return;
  const btn = document.getElementById('remove-btn-' + id);
  btn.disabled = true;
  try {
    await invoke('remove_mount', { id });
    showStatus('Mount removed', 'success');
    loadMounts();
  } catch (e) {
    btn.disabled = false;
    showStatus(formatError(e), 'error');
  }
}

async function addMount() {
  try {
    await invoke('open_wizard');
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

async function signOut() {
  const ok = await window.__TAURI__.dialog.confirm('Sign out? All mounts will stop.', { title: 'Sign Out', kind: 'warning' });
  if (!ok) return;
  const btn = document.getElementById('sign-out-btn');
  btn.disabled = true;
  btn.textContent = 'Signing out…';
  try {
    await invoke('sign_out');
    btn.disabled = false;
    btn.textContent = 'Sign Out';
    showStatus('Signed out', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Sign Out';
    showStatus(formatError(e), 'error');
  }
}

async function clearCache() {
  const btn = document.getElementById('btn-clear-cache');
  btn.disabled = true;
  btn.textContent = 'Clearing…';
  try {
    await invoke('clear_cache');
    btn.disabled = false;
    btn.textContent = 'Clear Cache';
    showStatus('Cache cleared', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Clear Cache';
    showStatus('Failed to clear cache: ' + formatError(e), 'error');
  }
}

// ---------------------------------------------------------------------------
// File Associations
// ---------------------------------------------------------------------------

/** Human-readable labels for handler source */
const SOURCE_LABELS = {
  override: 'Manual override',
  saved: 'Saved',
  discovered: 'Auto-detected',
  none: 'Not found',
};

function renderHandlerList(handlers) {
  const list = document.getElementById('handler-list');
  list.innerHTML = '';

  handlers.forEach(h => {
    const li = document.createElement('li');
    li.className = 'handler-item';

    const info = document.createElement('div');
    info.className = 'handler-info';

    const extEl = document.createElement('span');
    extEl.className = 'handler-ext';
    extEl.textContent = h.extension;
    info.appendChild(extEl);

    const nameEl = document.createElement('span');
    nameEl.className = 'handler-name';
    nameEl.textContent = h.handler_name || 'None';
    info.appendChild(nameEl);

    const sourceEl = document.createElement('span');
    sourceEl.className = 'handler-source badge';
    sourceEl.textContent = SOURCE_LABELS[h.source] || h.source;
    info.appendChild(sourceEl);

    const actions = document.createElement('div');
    actions.className = 'handler-actions';

    const overrideInput = document.createElement('input');
    overrideInput.type = 'text';
    overrideInput.className = 'handler-override-input';
    overrideInput.placeholder = 'Handler ID';
    overrideInput.value = h.source === 'override' ? h.handler_id : '';
    overrideInput.title = 'Enter handler identifier (ProgID, .desktop file, or bundle ID)';

    const setBtn = document.createElement('button');
    setBtn.className = 'btn-sm btn-secondary';
    setBtn.textContent = 'Set';
    setBtn.addEventListener('click', () => saveHandlerOverride(h.extension, overrideInput.value));

    actions.appendChild(overrideInput);
    actions.appendChild(setBtn);

    if (h.source === 'override') {
      const clearBtn = document.createElement('button');
      clearBtn.className = 'btn-sm btn-secondary';
      clearBtn.textContent = 'Clear';
      clearBtn.addEventListener('click', () => clearHandlerOverride(h.extension));
      actions.appendChild(clearBtn);
    }

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

async function loadFileHandlers() {
  try {
    const handlers = await invoke('get_file_handlers');
    renderHandlerList(handlers);
  } catch (e) {
    console.error(e);
    showStatus('Failed to load file handlers', 'error');
  }
}

async function redetectHandlers() {
  const btn = document.getElementById('btn-redetect');
  btn.disabled = true;
  btn.textContent = 'Detecting…';
  try {
    const handlers = await invoke('redetect_file_handlers');
    renderHandlerList(handlers);
    btn.disabled = false;
    btn.textContent = 'Re-detect Handlers';
    showStatus('Handlers re-detected', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Re-detect Handlers';
    showStatus(formatError(e), 'error');
  }
}

async function saveHandlerOverride(extension, handlerId) {
  if (!handlerId.trim()) {
    showStatus('Please enter a handler identifier', 'error');
    return;
  }
  try {
    await invoke('save_file_handler_override', { extension, handlerId: handlerId.trim() });
    showStatus('Handler override saved for ' + extension, 'success');
    loadFileHandlers();
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

async function clearHandlerOverride(extension) {
  try {
    await invoke('clear_file_handler_override', { extension });
    showStatus('Handler override cleared for ' + extension, 'success');
    loadFileHandlers();
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

loadSettings().catch(e => showStatus(formatError(e), 'error'));
loadMounts().catch(e => showStatus(formatError(e), 'error'));
loadFileHandlers().catch(e => showStatus(formatError(e), 'error'));

document.getElementById('save-settings').addEventListener('click', saveSettings);
document.getElementById('btn-add-mount').addEventListener('click', addMount);
document.getElementById('sign-out-btn').addEventListener('click', signOut);
document.getElementById('btn-clear-cache').addEventListener('click', clearCache);
document.getElementById('btn-redetect').addEventListener('click', redetectHandlers);

['auto-start', 'notifications', 'explorer-nav-pane', 'sync-interval', 'log-level'].forEach(id =>
  document.getElementById(id).addEventListener('change', checkDirty));
['cache-dir', 'cache-max-size', 'metadata-ttl'].forEach(id =>
  document.getElementById(id).addEventListener('input', checkDirty));

listen('refresh-settings', () => {
  loadSettings();
  loadMounts();
});
