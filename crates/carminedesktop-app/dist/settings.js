const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const state = {
  settings: {},
  mounts: [],
  libraries: [],
  librariesLoading: false,
  librariesError: null,
  handlers: [],
  offlinePins: [],
  activePanel: 'dashboard',
  dashboardStatus: null,
  recentActivity: [],
  recentErrors: [],
  cacheStats: null,
  activityExpanded: false,
  writebackExpanded: false,
};

function setState(patch) {
  Object.assign(state, patch);
  render();
}

// ---------------------------------------------------------------------------
// Dashboard helpers
// ---------------------------------------------------------------------------

function formatRelativeTime(isoString) {
  if (!isoString) return 'Jamais';
  const now = Date.now();
  const then = new Date(isoString).getTime();
  const diffSec = Math.floor((now - then) / 1000);
  if (diffSec < 60) return 'À l\'instant';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return 'Il y a ' + diffMin + 'm';
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return 'Il y a ' + diffHr + 'h';
  const diffDay = Math.floor(diffHr / 24);
  return 'Il y a ' + diffDay + 'j';
}

function formatBytes(bytes) {
  if (bytes === 0) return '0 B';
  const units = ['B', 'Ko', 'Mo', 'Go', 'To'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return (i === 0 ? val : val.toFixed(1)) + ' ' + units[i];
}

function truncatePath(fullPath, maxLen) {
  if (!fullPath) return '';
  if (!maxLen) maxLen = 40;
  if (fullPath.length <= maxLen) return fullPath;
  const parts = fullPath.split('/').filter(Boolean);
  if (parts.length <= 2) return fullPath;
  return '\u2026/' + parts.slice(-2).join('/');
}

function formatSyncStatus(drive) {
  if (!drive.online) return 'Hors-ligne';
  if (drive.syncState === 'error') return 'Erreur';
  if (drive.syncState === 'syncing') {
    const total = (drive.uploadQueue ? drive.uploadQueue.inFlight + drive.uploadQueue.queueDepth : 0);
    return total > 0 ? 'Synchro ' + total + ' fichiers' : 'Synchro en cours';
  }
  return 'À jour';
}

function aggregateUploadQueue(drives) {
  let inFlight = 0, queued = 0;
  if (!drives) return { inFlight: 0, queued: 0 };
  drives.forEach(function(d) {
    if (d.uploadQueue) {
      inFlight += d.uploadQueue.inFlight;
      queued += d.uploadQueue.queueDepth;
    }
  });
  return { inFlight: inFlight, queued: queued };
}

// ---------------------------------------------------------------------------
// Render functions
// ---------------------------------------------------------------------------

function renderNav() {
  document.querySelectorAll('.nav-item').forEach(item => {
    const panel = item.dataset.panel;
    item.classList.toggle('active', panel === state.activePanel);
    item.setAttribute('aria-selected', panel === state.activePanel ? 'true' : 'false');
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
  document.getElementById('account-email').textContent = s.account_display || 'Non connecté';
  
  const offlineTtl = document.getElementById('offline-ttl');
  if (offlineTtl) offlineTtl.value = String(s.offline_ttl_secs);
}

function renderMounts() {
  const list = document.getElementById('library-list');
  if (!list) return;
  list.innerHTML = '';

  const mountByDrive = {};
  state.mounts.forEach(function(m) {
    if (m.drive_id) mountByDrive[m.drive_id] = m;
  });

  const odMount = state.mounts.find(function(m) { return m.mount_type === 'drive'; });
  const driveInfo = state.settings.driveInfo;
  if (driveInfo || odMount) {
    const li = document.createElement('li');
    li.className = 'setting-row';

    const info = document.createElement('div');
    info.className = 'mount-info';
    const nameEl = document.createElement('div');
    nameEl.className = 'mount-name';
    nameEl.textContent = 'OneDrive';
    const pathEl = document.createElement('div');
    pathEl.className = 'mount-path';
    pathEl.textContent = odMount ? odMount.mount_point : 'Non monté';
    info.appendChild(nameEl);
    info.appendChild(pathEl);

    const actions = document.createElement('div');
    actions.className = 'mount-actions';
    const toggleLabel = document.createElement('label');
    toggleLabel.className = 'toggle-switch';
    const toggleInput = document.createElement('input');
    toggleInput.type = 'checkbox';
    toggleInput.checked = !!odMount;
    toggleInput.dataset.action = 'toggle-library';
    toggleInput.dataset.libraryType = 'onedrive';
    toggleInput.dataset.driveId = driveInfo ? driveInfo.id : (odMount ? odMount.drive_id : '');
    toggleInput.dataset.driveName = 'OneDrive';
    if (odMount) toggleInput.dataset.mountId = odMount.id;
    const toggleTrack = document.createElement('span');
    toggleTrack.className = 'toggle-track';
    toggleLabel.appendChild(toggleInput);
    toggleLabel.appendChild(toggleTrack);
    actions.appendChild(toggleLabel);

    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  }

  if (state.librariesLoading) {
    const loading = document.createElement('li');
    loading.className = 'mount-empty';
    loading.innerHTML = '<span class="spinner"></span> Chargement des bibliothèques\u2026';
    list.appendChild(loading);
    return;
  }

  if (state.librariesError) {
    const err = document.createElement('li');
    err.className = 'mount-empty';
    err.textContent = 'Impossible de charger les bibliothèques';
    list.appendChild(err);
    return;
  }

  state.libraries.forEach(function(lib) {
    const existingMount = mountByDrive[lib.id];
    const li = document.createElement('li');
    li.className = 'setting-row';

    const info = document.createElement('div');
    info.className = 'mount-info';
    const nameEl = document.createElement('div');
    nameEl.className = 'mount-name';
    nameEl.textContent = lib.name;
    const pathEl = document.createElement('div');
    pathEl.className = 'mount-path';
    pathEl.textContent = existingMount ? existingMount.mount_point : 'Non monté';
    info.appendChild(nameEl);
    info.appendChild(pathEl);

    const actions = document.createElement('div');
    actions.className = 'mount-actions';
    const toggleLabel = document.createElement('label');
    toggleLabel.className = 'toggle-switch';
    const toggleInput = document.createElement('input');
    toggleInput.type = 'checkbox';
    toggleInput.checked = !!existingMount;
    toggleInput.dataset.action = 'toggle-library';
    toggleInput.dataset.libraryType = 'sharepoint';
    toggleInput.dataset.driveId = lib.id;
    toggleInput.dataset.driveName = lib.name;
    if (existingMount) toggleInput.dataset.mountId = existingMount.id;
    const toggleTrack = document.createElement('span');
    toggleTrack.className = 'toggle-track';
    toggleLabel.appendChild(toggleInput);
    toggleLabel.appendChild(toggleTrack);
    actions.appendChild(toggleLabel);

    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });

  if (state.libraries.length === 0 && !odMount && !driveInfo) {
    const empty = document.createElement('li');
    empty.className = 'mount-empty';
    empty.textContent = 'Aucune bibliothèque disponible';
    list.appendChild(empty);
  }
}

function formatTimeRemaining(expiresAt) {
  const now = new Date();
  const expires = new Date(expiresAt + 'Z');
  const diffMs = expires - now;
  if (diffMs <= 0) return { text: 'Expiré', expired: true };
  const hours = Math.floor(diffMs / 3600000);
  const days = Math.floor(hours / 24);
  if (days > 0) return { text: 'Reste ' + days + 'j ' + (hours % 24) + 'h', expired: false };
  const mins = Math.floor((diffMs % 3600000) / 60000);
  if (hours > 0) return { text: 'Reste ' + hours + 'h ' + mins + 'm', expired: false };
  return { text: 'Reste ' + mins + 'm', expired: false };
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
    const displayName = pin.folder_name === 'root' ? pin.mount_name : pin.folder_name;
    nameEl.textContent = displayName;
    const metaEl = document.createElement('div');
    metaEl.className = 'pin-meta';
    const remaining = formatTimeRemaining(pin.expires_at);
    const expirySpan = document.createElement('span');
    expirySpan.className = 'pin-expiry' + (remaining.expired ? ' expired' : '');
    expirySpan.textContent = remaining.text;
    metaEl.appendChild(document.createTextNode(pin.mount_name + ' \u00B7 '));
    metaEl.appendChild(expirySpan);

    const cs = state.cacheStats;
    if (cs && cs.pinnedItems) {
      const health = cs.pinnedItems.find(function(h) {
        return h.itemId === pin.item_id && h.driveId === pin.drive_id;
      });
      if (health) {
        const badge = document.createElement('span');
        if (health.totalFiles === 0) {
          badge.className = 'health-badge partial';
          badge.textContent = 'analyse\u2026';
        } else {
          const statusMap = { 'downloaded': 'disponible', 'partial': 'partiel', 'stale': 'obsolète' };
          badge.className = 'health-badge ' + health.status;
          badge.textContent = statusMap[health.status] || health.status;
        }
        metaEl.appendChild(document.createTextNode(' '));
        metaEl.appendChild(badge);
        if (health.totalFiles > 0) {
          const fileCount = document.createElement('span');
          fileCount.className = 'pin-file-count';
          fileCount.textContent = ' \u00B7 ' + health.cachedFiles + '/' + health.totalFiles + ' fichiers';
          metaEl.appendChild(fileCount);
        }
      }
    }
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
    removeBtn.title = 'Supprimer l\'accès hors-ligne';
    removeBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6L6 18M6 6l12 12"/></svg>';

    actions.appendChild(removeBtn);
    li.appendChild(info);
    li.appendChild(actions);
    list.appendChild(li);
  });

  if (state.offlinePins.length === 0) {
    const empty = document.createElement('li');
    empty.className = 'pin-empty';
    empty.textContent = 'Aucun dossier épinglé pour le mode hors-ligne';
    list.appendChild(empty);
  }
}

function renderDashboard() {
  const banner = document.getElementById('auth-banner');
  if (banner) {
    const ds = state.dashboardStatus;
    if (ds && ds.authDegraded) {
      banner.style.display = 'flex';
      banner.innerHTML = '';
      const left = document.createElement('div');
      left.className = 'auth-banner-left';
      left.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="auth-banner-icon"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>';
      const msg = document.createElement('span');
      msg.textContent = 'L\'authentification nécessite votre attention.';
      left.appendChild(msg);
      banner.appendChild(left);
      const btn = document.createElement('button');
      btn.className = 'btn-ghost btn-sm';
      btn.dataset.action = 'dashboard-sign-in';
      btn.textContent = 'Se connecter';
      banner.appendChild(btn);
    } else {
      banner.style.display = 'none';
    }
  }

  const cardsContainer = document.getElementById('drive-cards');
  if (cardsContainer) {
    cardsContainer.innerHTML = '';
    const ds = state.dashboardStatus;
    if (ds && ds.drives && ds.drives.length > 0) {
      ds.drives.forEach(function(drive) {
        const card = document.createElement('div');
        card.className = 'drive-card';

        const header = document.createElement('div');
        header.className = 'drive-card-header';
        const dot = document.createElement('span');
        dot.className = 'status-dot';
        if (!drive.online) { dot.classList.add('offline'); }
        else if (drive.syncState === 'error') { dot.classList.add('error'); }
        else if (drive.syncState === 'syncing') { dot.classList.add('syncing'); }
        else { dot.classList.add('ok'); }
        header.appendChild(dot);
        const name = document.createElement('div');
        name.className = 'drive-card-name';
        name.textContent = drive.name;
        header.appendChild(name);

        const status = document.createElement('div');
        status.className = 'drive-card-status';
        status.textContent = formatSyncStatus(drive);

        const lastSync = document.createElement('div');
        lastSync.className = 'drive-card-last-sync';
        lastSync.textContent = 'Dernière synchro : ' + formatRelativeTime(drive.lastSynced);

        card.appendChild(header);
        card.appendChild(status);
        card.appendChild(lastSync);
        cardsContainer.appendChild(card);
      });
    } else {
      const empty = document.createElement('div');
      empty.className = 'mount-empty';
      empty.textContent = 'Aucun lecteur monté';
      cardsContainer.appendChild(empty);
    }
  }

  const uploadSummary = document.getElementById('upload-summary');
  const uploadDetail = document.getElementById('upload-detail');
  if (uploadSummary) {
    const ds = state.dashboardStatus;
    const agg = aggregateUploadQueue(ds ? ds.drives : []);
    const total = agg.inFlight + agg.queued;
    if (total > 0) {
      uploadSummary.style.display = 'flex';
      uploadSummary.innerHTML = '';
      const arrow = document.createElement('span');
      arrow.className = 'disclosure-arrow' + (state.writebackExpanded ? ' expanded' : '');
      arrow.textContent = '\u25B6';
      uploadSummary.appendChild(arrow);
      const text = document.createElement('span');
      const parts = [];
      if (agg.inFlight > 0) parts.push(agg.inFlight + ' en envoi');
      if (agg.queued > 0) parts.push(agg.queued + ' en attente');
      text.textContent = parts.join(', ');
      uploadSummary.appendChild(text);
      uploadSummary.dataset.action = 'toggle-writeback-expanded';
    } else {
      uploadSummary.style.display = 'none';
    }

    if (uploadDetail) {
      const cs = state.cacheStats;
      if (state.writebackExpanded && cs && cs.writebackQueue && cs.writebackQueue.length > 0) {
        uploadDetail.style.display = 'block';
        uploadDetail.innerHTML = '';
        cs.writebackQueue.forEach(function(entry) {
          const row = document.createElement('div');
          row.className = 'upload-detail-file';
          row.textContent = entry.fileName;
          uploadDetail.appendChild(row);
        });
      } else {
        uploadDetail.style.display = 'none';
      }
    }
  }

  const activityList = document.getElementById('activity-list');
  if (activityList) {
    activityList.innerHTML = '';
    const entries = state.recentActivity;
    if (entries && entries.length > 0) {
      const limit = state.activityExpanded ? entries.length : Math.min(entries.length, 10);
      for (let i = 0; i < limit; i++) {
        const entry = entries[i];
        const li = document.createElement('li');
        li.className = 'activity-row';

        const typeMap = { 'synced': 'synchronisé', 'uploaded': 'envoyé', 'deleted': 'supprimé', 'conflict': 'conflit' };
        const tag = document.createElement('span');
        tag.className = 'activity-tag ' + entry.activityType;
        tag.textContent = typeMap[entry.activityType] || entry.activityType;

        const name = document.createElement('span');
        name.className = 'activity-name';
        name.textContent = truncatePath(entry.filePath);

        const time = document.createElement('span');
        time.className = 'activity-time';
        time.textContent = formatRelativeTime(entry.timestamp);

        li.appendChild(tag);
        li.appendChild(name);
        li.appendChild(time);
        activityList.appendChild(li);
      }
      if (entries.length > 10) {
        const showMore = document.createElement('li');
        showMore.className = 'activity-show-more';
        const link = document.createElement('button');
        link.className = 'btn-link';
        link.dataset.action = 'toggle-activity-expanded';
        link.textContent = state.activityExpanded ? 'Voir moins' : 'Tout voir (' + entries.length + ')';
        showMore.appendChild(link);
        activityList.appendChild(showMore);
      }
    } else {
      const empty = document.createElement('li');
      empty.className = 'activity-empty';
      empty.textContent = 'Aucune activité récente';
      activityList.appendChild(empty);
    }
  }

  const errorsHeading = document.getElementById('errors-heading');
  const errorList = document.getElementById('error-list');
  if (errorsHeading) {
    const count = state.recentErrors ? state.recentErrors.length : 0;
    errorsHeading.textContent = count > 0 ? 'Erreurs (' + count + ')' : 'Erreurs';
  }
  if (errorList) {
    errorList.innerHTML = '';
    const errors = state.recentErrors;
    if (errors && errors.length > 0) {
      errors.forEach(function(err) {
        const entry = document.createElement('div');
        entry.className = 'error-entry' + (err.errorType === 'conflict' ? ' conflict' : '');

        const header = document.createElement('div');
        header.className = 'error-header';
        const fileEl = document.createElement('span');
        fileEl.className = 'error-file';
        fileEl.textContent = err.fileName || 'Fichier inconnu';
        const typeEl = document.createElement('span');
        typeEl.className = 'error-type';
        const errorTypeMap = { 'conflict': 'conflit', 'writeback_failed': 'échec écriture', 'upload_failed': 'échec envoi' };
        typeEl.textContent = ' \u2013 ' + (errorTypeMap[err.errorType] || err.errorType || 'erreur');
        const timeEl = document.createElement('span');
        timeEl.className = 'error-time';
        timeEl.textContent = formatRelativeTime(err.timestamp);
        header.appendChild(fileEl);
        header.appendChild(typeEl);
        header.appendChild(timeEl);
        entry.appendChild(header);

        if (err.message) {
          const msgEl = document.createElement('div');
          msgEl.className = 'error-message';
          msgEl.textContent = err.message;
          entry.appendChild(msgEl);
        }
        errorList.appendChild(entry);
      });
    } else {
      const empty = document.createElement('div');
      empty.className = 'error-empty';
      empty.textContent = 'Aucune erreur';
      errorList.appendChild(empty);
    }
  }

  const cacheSection = document.getElementById('cache-section');
  if (cacheSection) {
    cacheSection.innerHTML = '';
    const cs = state.cacheStats;
    const usedBytes = cs ? cs.diskUsedBytes : 0;
    const maxBytes = cs ? cs.diskMaxBytes : 0;
    const pct = maxBytes > 0 ? Math.min((usedBytes / maxBytes) * 100, 100) : 0;
    const barColor = pct >= 90 ? 'red' : (pct >= 70 ? 'amber' : 'green');

    const bar = document.createElement('div');
    bar.className = 'cache-bar';
    const fill = document.createElement('div');
    fill.className = 'cache-bar-fill ' + barColor;
    fill.style.width = pct + '%';
    bar.appendChild(fill);
    cacheSection.appendChild(bar);

    const text = document.createElement('div');
    text.className = 'cache-text';
    text.textContent = formatBytes(usedBytes) + ' / ' + formatBytes(maxBytes);
    cacheSection.appendChild(text);

    const pins = cs ? cs.pinnedItems : [];
    if (pins && pins.length > 0) {
      const summary = document.createElement('div');
      summary.className = 'pin-summary';
      const counts = { downloaded: 0, partial: 0, stale: 0 };
      pins.forEach(function(p) { counts[p.status] = (counts[p.status] || 0) + 1; });
      const statusParts = [];
      if (counts.downloaded > 0) statusParts.push(counts.downloaded + ' Disponibles');
      if (counts.partial > 0) statusParts.push(counts.partial + ' Partiels');
      if (counts.stale > 0) statusParts.push(counts.stale + ' Obsolètes');
      summary.textContent = pins.length + ' dossiers épinglés \u00B7 ' + statusParts.join(', ');
      cacheSection.appendChild(summary);
    } else {
      const empty = document.createElement('div');
      empty.className = 'pin-summary-empty';
      empty.textContent = 'Aucun dossier hors-ligne';
      cacheSection.appendChild(empty);
    }
  }
}

function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderOfflinePins();
  renderDashboard();
}

let _renderRAF = null;
function scheduleRender() {
  if (_renderRAF) return;
  _renderRAF = requestAnimationFrame(function() {
    _renderRAF = null;
    render();
  });
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

async function saveSettings() {
  try {
    await invoke('save_settings', {
      autoStart: document.getElementById('auto-start').checked,
      notifications: document.getElementById('notifications').checked,
      syncIntervalSecs: parseInt(document.getElementById('sync-interval').value),
      explorerNavPane: true,
      cacheDir: document.getElementById('cache-dir').value || null,
      cacheMaxSize: document.getElementById('cache-max-size').value,
      metadataTtlSecs: 60,
      logLevel: 'info',
      offlineTtlSecs: parseInt(document.getElementById('offline-ttl').value) || null,
      offlineMaxFolderSize: '5GB',
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

async function toggleLibraryOn(libraryType, driveId, driveName) {
  try {
    const mountRoot = await invoke('get_default_mount_root');
    let mountPoint, siteId, siteName, libraryName;

    if (libraryType === 'sharepoint') {
      siteName = state.settings.primarySiteName || null;
      libraryName = driveName;
      mountPoint = mountRoot + '/' + sanitizePath(driveName);
      siteId = state.settings.primarySiteId || null;
    } else {
      mountPoint = mountRoot + '/OneDrive';
    }

    await invoke('add_mount', {
      mountType: libraryType === 'sharepoint' ? 'sharepoint' : 'drive',
      mountPoint: mountPoint,
      driveId: driveId,
      siteId: siteId || null,
      siteName: siteName || null,
      libraryName: libraryName || null,
    });
    showStatus(driveName + ' activé', 'success');
    const mounts = await invoke('list_mounts');
    setState({ mounts: mounts });
  } catch (e) {
    showStatus(formatError(e), 'error');
    try {
      const mounts = await invoke('list_mounts');
      setState({ mounts: mounts });
    } catch (_) { render(); }
  }
}

async function toggleLibraryOff(mountId, driveName) {
  try {
    await invoke('remove_mount', { id: mountId });
    showStatus(driveName + ' désactivé', 'success');
    const mounts = await invoke('list_mounts');
    setState({ mounts: mounts });
  } catch (e) {
    showStatus(formatError(e), 'error');
    try {
      const mounts = await invoke('list_mounts');
      setState({ mounts: mounts });
    } catch (_) { render(); }
  }
}

async function loadLibraries() {
  setState({ librariesLoading: true, librariesError: null });
  try {
    const [libraries, driveInfo, siteInfo] = await Promise.all([
      invoke('list_primary_site_libraries').catch(function() { return []; }),
      invoke('get_drive_info').catch(function() { return null; }),
      invoke('get_primary_site_info').catch(function() { return null; }),
    ]);
    const s = Object.assign({}, state.settings);
    if (driveInfo) s.driveInfo = driveInfo;
    if (siteInfo) {
      s.primarySiteId = siteInfo.site_id;
      s.primarySiteName = siteInfo.site_name;
    }
    setState({ libraries: libraries, librariesLoading: false, settings: s });
  } catch (e) {
    setState({ librariesLoading: false, librariesError: formatError(e) });
  }
}

async function signOut() {
  const ok = await window.__TAURI__.dialog.confirm('Voulez-vous vous déconnecter ? Tous les lecteurs seront démontés.', { title: 'Déconnexion', kind: 'warning' });
  if (!ok) return;
  const btn = document.getElementById('sign-out-btn');
  btn.disabled = true;
  btn.textContent = 'Déconnexion en cours\u2026';
  try {
    await invoke('sign_out');
    btn.disabled = false;
    btn.textContent = 'Déconnexion';
    showStatus('Déconnecté', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = 'Déconnexion';
    showStatus(formatError(e), 'error');
  }
}

async function clearCache() {
  const btn = document.getElementById('btn-clear-cache');
  const origText = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Vidage\u2026';
  try {
    await invoke('clear_cache');
    btn.disabled = false;
    btn.textContent = origText;
    showStatus('Cache vidé', 'success');
  } catch (e) {
    btn.disabled = false;
    btn.textContent = origText;
    showStatus('Échec du vidage du cache : ' + formatError(e), 'error');
  }
}

async function removeOfflinePin(driveId, itemId, name) {
  try {
    await invoke('remove_offline_pin', { driveId, itemId });
    showStatus('Épinglage supprimé pour ' + name, 'success');
    const offlinePins = await invoke('list_offline_pins');
    setState({ offlinePins });
  } catch (e) {
    showStatus(formatError(e), 'error');
  }
}

async function refreshDashboardData() {
  try {
    const [dashboardStatus, cacheStats] = await Promise.all([
      invoke('get_dashboard_status'),
      invoke('get_cache_stats'),
    ]);
    Object.assign(state, { dashboardStatus, cacheStats });
    render();
  } catch (_) { }
}

async function refreshOfflineData() {
  try {
    const [offlinePins, cacheStats] = await Promise.all([
      invoke('list_offline_pins'),
      invoke('get_cache_stats'),
    ]);
    Object.assign(state, { offlinePins, cacheStats });
    render();
  } catch (_) { }
}

function refreshPanelData(panel) {
  if (panel === 'dashboard') refreshDashboardData();
  else if (panel === 'offline') refreshOfflineData();
  else if (panel === 'mounts') loadLibraries();
}

async function init() {
  try {
    const [settings, mounts, offlinePins, dashboardStatus, recentErrors, recentActivity, cacheStats] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
      invoke('list_offline_pins'),
      invoke('get_dashboard_status'),
      invoke('get_recent_errors'),
      invoke('get_activity_feed'),
      invoke('get_cache_stats'),
    ]);
    recentActivity.reverse();
    recentErrors.reverse();
    setState({ settings, mounts, offlinePins, dashboardStatus, recentErrors, recentActivity, cacheStats });
    document.title = 'Paramètres Carmine';
    document.getElementById('app-version').textContent = 'Version ' + settings.app_version;
    loadLibraries();
  } catch (e) {
    showStatus(formatError(e), 'error');
  }

  const navItems = Array.from(document.querySelectorAll('.nav-item'));
  navItems.forEach(item => {
    item.addEventListener('click', () => {
      const panel = item.dataset.panel;
      setState({ activePanel: panel });
      refreshPanelData(panel);
    });
  });

  ['auto-start', 'notifications', 'sync-interval', 'offline-ttl'].forEach(id =>
    document.getElementById(id).addEventListener('change', saveSettings));
  ['cache-dir', 'cache-max-size'].forEach(id =>
    document.getElementById(id).addEventListener('input', debouncedSave));

  document.getElementById('sign-out-btn').addEventListener('click', signOut);
  document.getElementById('btn-clear-cache').addEventListener('click', clearCache);

  document.querySelector('.main-content').addEventListener('click', async (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    const action = target.dataset.action;
    if (action === 'remove-pin') await removeOfflinePin(target.dataset.driveId, target.dataset.itemId, target.dataset.name);
    else if (action === 'toggle-activity-expanded') {
      setState({ activityExpanded: !state.activityExpanded });
    }
    else if (action === 'toggle-writeback-expanded') {
      setState({ writebackExpanded: !state.writebackExpanded });
    }
    else if (action === 'dashboard-sign-in') {
      try {
        await invoke('sign_out');
        showStatus('Veuillez vous reconnecter', 'info');
      } catch (e) {
        showStatus(formatError(e), 'error');
      }
    }
  });

  document.querySelector('.main-content').addEventListener('change', async (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    if (target.dataset.action === 'toggle-library') {
      const isOn = target.checked;
      if (isOn) {
        await toggleLibraryOn(target.dataset.libraryType, target.dataset.driveId, target.dataset.driveName);
      } else {
        await toggleLibraryOff(target.dataset.mountId, target.dataset.driveName);
      }
    }
  });

  listen('obs-event', function(event) {
    const p = event.payload;
    const ds = state.dashboardStatus;
    if (!ds) return;
    switch (p.type) {
      case 'syncStateChanged': {
        const drive = ds.drives.find(function(d) { return d.driveId === p.driveId; });
        if (drive) { drive.syncState = p.state; if (p.state !== 'syncing') refreshDashboardData(); }
        break;
      }
      case 'onlineStateChanged': {
        const drive = ds.drives.find(function(d) { return d.driveId === p.driveId; });
        if (drive) drive.online = p.online;
        break;
      }
      case 'authStateChanged': { ds.authDegraded = p.degraded; break; }
      case 'error': {
        state.recentErrors.unshift({ fileName: p.fileName, errorType: p.errorType, message: p.message, timestamp: p.timestamp });
        if (state.recentErrors.length > 100) state.recentErrors.length = 100;
        break;
      }
      case 'activity': {
        state.recentActivity.unshift({ filePath: p.filePath, activityType: p.activityType, timestamp: p.timestamp });
        if (state.recentActivity.length > 500) state.recentActivity.length = 500;
        break;
      }
    }
    scheduleRender();
  });

  setInterval(function() { refreshPanelData(state.activePanel); }, 30000);
}

init();
