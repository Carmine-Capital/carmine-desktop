const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Render functions
// ---------------------------------------------------------------------------

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

function renderMounts() {
  const list = document.getElementById('mount-list');
  list.innerHTML = '';
  state.mounts.forEach(m => {
    const li = document.createElement('li');
    li.className = 'setting-row' + (m.enabled ? '' : ' mount-disabled');

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

function renderHandlers() {
  const list = document.getElementById('handler-list');
  list.innerHTML = '';
  state.handlers.forEach(h => {
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

    const actions = document.createElement('div');
    actions.className = 'setting-control';

    const overrideBtn = document.createElement('button');
    overrideBtn.className = 'btn-ghost btn-sm';
    overrideBtn.textContent = h.source === 'override' ? 'Change' : 'Override';
    overrideBtn.dataset.action = 'override-handler';
    overrideBtn.dataset.ext = h.extension;
    overrideBtn.dataset.source = h.source;
    overrideBtn.dataset.handlerId = h.handler_id || '';

    actions.appendChild(overrideBtn);
    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });
  if (state.handlers.length === 0) {
    const empty = document.createElement('li');
    empty.className = 'handler-empty';
    empty.textContent = 'No file handlers found';
    list.appendChild(empty);
  }
}

function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderHandlers();
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

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

let _saveTimer = null;
function debouncedSave() {
  clearTimeout(_saveTimer);
  _saveTimer = setTimeout(saveSettings, 500);
}

async function toggleMount(id) {
  try {
    await invoke('toggle_mount', { id });
    showStatus('Mount updated', 'success');
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
  try {
    const mounts = await invoke('list_mounts');
    setState({ mounts });
  } catch (_) {
    renderMounts();
  }
}

async function removeMount(id) {
  const ok = await window.__TAURI__.dialog.confirm('Remove this mount? This cannot be undone.', { title: 'Remove Mount', kind: 'warning' });
  if (!ok) return;
  try {
    await invoke('remove_mount', { id });
    showStatus('Mount removed', 'success');
    const mounts = await invoke('list_mounts');
    setState({ mounts });
  } catch (e) {
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
  btn.textContent = 'Signing out\u2026';
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
  const origText = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Clearing\u2026';
  try {
    await invoke('clear_cache');
    btn.disabled = false;
    btn.textContent = origText;
    showStatus('Cache cleared', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = origText;
    showStatus('Failed to clear cache: ' + formatError(e), 'error');
  }
}

// ---------------------------------------------------------------------------
// File Associations
// ---------------------------------------------------------------------------

async function redetectHandlers() {
  const btn = document.getElementById('btn-redetect');
  btn.disabled = true;
  btn.textContent = 'Detecting\u2026';
  try {
    const handlers = await invoke('redetect_file_handlers');
    setState({ handlers });
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
    const handlers = await invoke('get_file_handlers');
    setState({ handlers });
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

async function clearHandlerOverride(extension) {
  try {
    await invoke('clear_file_handler_override', { extension });
    showStatus('Handler override cleared for ' + extension, 'success');
    const handlers = await invoke('get_file_handlers');
    setState({ handlers });
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

// -- Delegation helpers for handler override expand/collapse --

function showOverrideInput(target) {
  const row = target.closest('.setting-row');
  const actions = row.querySelector('.setting-control');
  const ext = target.dataset.ext;
  const isOverride = target.dataset.source === 'override';

  actions.innerHTML = '';
  const input = document.createElement('input');
  input.type = 'text';
  input.className = 'handler-override-input';
  input.placeholder = 'Handler ID';
  input.value = isOverride ? target.dataset.handlerId : '';

  const setBtn = document.createElement('button');
  setBtn.className = 'btn-ghost btn-sm';
  setBtn.textContent = 'Set';
  setBtn.dataset.action = 'set-override';
  setBtn.dataset.ext = ext;

  actions.appendChild(input);
  actions.appendChild(setBtn);

  if (isOverride) {
    const clearBtn = document.createElement('button');
    clearBtn.className = 'btn-link btn-sm';
    clearBtn.textContent = 'Clear';
    clearBtn.dataset.action = 'clear-override';
    clearBtn.dataset.ext = ext;
    actions.appendChild(clearBtn);
  }

  input.focus();
}

async function setOverride(target) {
  const row = target.closest('.setting-row');
  const input = row.querySelector('.handler-override-input');
  if (input) await saveHandlerOverride(target.dataset.ext, input.value);
}

async function clearOverride(target) {
  await clearHandlerOverride(target.dataset.ext);
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

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

  // Delegation for dynamically rendered mount and handler rows
  document.querySelector('.main-content').addEventListener('click', async (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    const action = target.dataset.action;
    if (action === 'remove-mount') await removeMount(target.dataset.id);
    else if (action === 'override-handler') showOverrideInput(target);
    else if (action === 'set-override') await setOverride(target);
    else if (action === 'clear-override') await clearOverride(target);
  });

  document.querySelector('.main-content').addEventListener('change', async (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    if (target.dataset.action === 'toggle-mount') await toggleMount(target.dataset.id);
  });

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
