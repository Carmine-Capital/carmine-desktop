const { invoke } = window.__TAURI__.core;

const tabs = Array.from(document.querySelectorAll('.tab'));

function activateTab(tab) {
  tabs.forEach(t => {
    t.classList.remove('active');
    t.setAttribute('aria-selected', 'false');
    t.setAttribute('tabindex', '-1');
  });
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  tab.classList.add('active');
  tab.setAttribute('aria-selected', 'true');
  tab.setAttribute('tabindex', '0');
  document.getElementById(tab.dataset.panel).classList.add('active');
  tab.focus();
}

tabs.forEach(tab => {
  tab.addEventListener('click', () => activateTab(tab));
  tab.addEventListener('keydown', (e) => {
    const idx = tabs.indexOf(tab);
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      activateTab(tabs[(idx + 1) % tabs.length]);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      activateTab(tabs[(idx - 1 + tabs.length) % tabs.length]);
    } else if (e.key === 'Home') {
      e.preventDefault();
      activateTab(tabs[0]);
    } else if (e.key === 'End') {
      e.preventDefault();
      activateTab(tabs[tabs.length - 1]);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activateTab(tab);
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
    document.getElementById('account-email').textContent = s.account_display || 'Not signed in';
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
      li.className = 'mount-item';

      const info = document.createElement('div');
      const nameEl = document.createElement('div');
      nameEl.className = 'mount-name';
      nameEl.textContent = m.name;
      const pathEl = document.createElement('div');
      pathEl.className = 'mount-path';
      pathEl.textContent = m.mount_point;
      info.appendChild(nameEl);
      info.appendChild(pathEl);

      const actions = document.createElement('div');
      const toggleBtn = document.createElement('button');
      toggleBtn.id = 'toggle-btn-' + m.id;
      toggleBtn.textContent = m.enabled ? 'Disable' : 'Enable';
      toggleBtn.onclick = () => toggleMount(m.id);
      const removeBtn = document.createElement('button');
      removeBtn.id = 'remove-btn-' + m.id;
      removeBtn.className = 'btn-danger';
      removeBtn.textContent = 'Remove';
      removeBtn.onclick = () => removeMount(m.id);
      actions.className = 'mount-actions';
      actions.appendChild(toggleBtn);
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

async function saveGeneral() {
  const btn = document.querySelector('#general .actions button');
  btn.disabled = true;
  btn.textContent = 'Saving\u2026';
  try {
    await invoke('save_settings', {
      autoStart: document.getElementById('auto-start').checked,
      notifications: document.getElementById('notifications').checked,
      syncIntervalSecs: parseInt(document.getElementById('sync-interval').value),
    });
    btn.disabled = false;
    btn.textContent = 'Save';
    showStatus('Settings saved', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Save';
    showStatus(e, 'error');
  }
}

async function saveAdvanced() {
  const btn = document.getElementById('btn-save-advanced');
  btn.disabled = true;
  btn.textContent = 'Saving\u2026';
  try {
    await invoke('save_settings', {
      cacheDir: document.getElementById('cache-dir').value || null,
      cacheMaxSize: document.getElementById('cache-max-size').value,
      metadataTtlSecs: parseInt(document.getElementById('metadata-ttl').value),
      logLevel: document.getElementById('log-level').value,
    });
    btn.disabled = false;
    btn.textContent = 'Save';
    showStatus('Settings saved', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Save';
    showStatus(e, 'error');
  }
}

async function toggleMount(id) {
  const btn = document.getElementById('toggle-btn-' + id);
  const origLabel = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Updating\u2026';
  try {
    await invoke('toggle_mount', { id });
    showStatus('Mount updated', 'success');
    loadMounts();
  } catch (e) {
    btn.disabled = false;
    btn.textContent = origLabel;
    showStatus(e, 'error');
  }
}

async function removeMount(id) {
  const ok = await window.__TAURI__.dialog.confirm('Remove this mount? This cannot be undone.', { title: 'Remove Mount', kind: 'warning' });
  if (!ok) return;
  const btn = document.getElementById('remove-btn-' + id);
  btn.disabled = true;
  btn.textContent = 'Removing\u2026';
  try {
    await invoke('remove_mount', { id });
    showStatus('Mount removed', 'success');
    loadMounts();
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Remove';
    showStatus(e, 'error');
  }
}

async function addMount() {
  try {
    await invoke('open_wizard');
  } catch (e) {
    showStatus(e.toString(), 'error');
  }
}

async function signOut() {
  const ok = await window.__TAURI__.dialog.confirm('Sign out? All mounts will stop.', { title: 'Sign Out', kind: 'warning' });
  if (!ok) return;
  const btn = document.getElementById('btn-sign-out');
  btn.disabled = true;
  btn.textContent = 'Signing out\u2026';
  try {
    await invoke('sign_out');
    showStatus('Signed out', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Sign Out';
    showStatus(e, 'error');
  }
}

async function clearCache() {
  const btn = document.getElementById('btn-clear-cache');
  btn.disabled = true;
  btn.textContent = 'Clearing\u2026';
  try {
    await invoke('clear_cache');
    btn.disabled = false;
    btn.textContent = 'Clear Cache';
    showStatus('Cache cleared', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Clear Cache';
    showStatus('Failed to clear cache: ' + e, 'error');
  }
}

loadSettings();
loadMounts();

document.getElementById('btn-save-general').addEventListener('click', saveGeneral);
document.getElementById('btn-save-advanced').addEventListener('click', saveAdvanced);
document.getElementById('btn-add-mount').addEventListener('click', addMount);
document.getElementById('btn-sign-out').addEventListener('click', signOut);
document.getElementById('btn-clear-cache').addEventListener('click', clearCache);
