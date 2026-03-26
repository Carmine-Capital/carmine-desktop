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
  if (!isoString) return 'Never';
  const now = Date.now();
  const then = new Date(isoString).getTime();
  const diffSec = Math.floor((now - then) / 1000);
  if (diffSec < 60) return 'Just now';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return diffMin + 'm ago';
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return diffHr + 'h ago';
  const diffDay = Math.floor(diffHr / 24);
  return diffDay + 'd ago';
}

function formatBytes(bytes) {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
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
  if (!drive.online) return 'Offline';
  if (drive.syncState === 'error') return 'Error';
  if (drive.syncState === 'syncing') {
    const total = (drive.uploadQueue ? drive.uploadQueue.inFlight + drive.uploadQueue.queueDepth : 0);
    return total > 0 ? 'Syncing ' + total + ' files' : 'Syncing';
  }
  return 'Up to date';
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
  const setDefaultBtn = document.getElementById('btn-set-default');
  if (setDefaultBtn && s.platform === 'windows') {
    setDefaultBtn.style.display = '';
  }
  const offlineTtl = document.getElementById('offline-ttl');
  if (offlineTtl) offlineTtl.value = String(s.offline_ttl_secs);
  const offlineMaxSize = document.getElementById('offline-max-size');
  if (offlineMaxSize) offlineMaxSize.value = s.offline_max_folder_size;
}

function renderMounts() {
  const list = document.getElementById('library-list');
  if (!list) return;
  list.innerHTML = '';

  // Build a lookup: drive_id -> mount for currently mounted items
  const mountByDrive = {};
  state.mounts.forEach(function(m) {
    if (m.drive_id) mountByDrive[m.drive_id] = m;
  });

  // OneDrive toggle (always shown if user has a drive)
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
    pathEl.textContent = odMount ? odMount.mount_point : 'Not mounted';
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

  // Loading state
  if (state.librariesLoading) {
    const loading = document.createElement('li');
    loading.className = 'mount-empty';
    loading.innerHTML = '<span class="spinner"></span> Loading libraries\u2026';
    list.appendChild(loading);
    return;
  }

  // Error state
  if (state.librariesError) {
    const err = document.createElement('li');
    err.className = 'mount-empty';
    err.textContent = 'Could not load libraries';
    list.appendChild(err);
    return;
  }

  // SharePoint libraries
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
    pathEl.textContent = existingMount ? existingMount.mount_point : 'Not mounted';
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
    empty.textContent = 'No libraries available';
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
    info.className = 'handler-info';

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
    // Health badge from cacheStats
    const cs = state.cacheStats;
    if (cs && cs.pinnedItems) {
      const health = cs.pinnedItems.find(function(h) {
        return h.itemId === pin.item_id && h.driveId === pin.drive_id;
      });
      if (health) {
        const badge = document.createElement('span');
        if (health.totalFiles === 0) {
          badge.className = 'health-badge partial';
          badge.textContent = 'scanning';
        } else {
          badge.className = 'health-badge ' + health.status;
          badge.textContent = health.status;
        }
        metaEl.appendChild(document.createTextNode(' '));
        metaEl.appendChild(badge);
        if (health.totalFiles > 0) {
          const fileCount = document.createElement('span');
          fileCount.className = 'pin-file-count';
          fileCount.textContent = health.cachedFiles + '/' + health.totalFiles + ' files';
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

function renderDashboard() {
  // Auth banner
  const banner = document.getElementById('auth-banner');
  if (banner) {
    const ds = state.dashboardStatus;
    if (ds && ds.authDegraded) {
      banner.style.display = '';
      banner.innerHTML = '';
      const left = document.createElement('div');
      left.className = 'auth-banner-left';
      const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      icon.setAttribute('width', '16');
      icon.setAttribute('height', '16');
      icon.setAttribute('viewBox', '0 0 24 24');
      icon.setAttribute('fill', 'none');
      icon.setAttribute('stroke', 'currentColor');
      icon.setAttribute('stroke-width', '2');
      icon.setAttribute('stroke-linecap', 'round');
      icon.setAttribute('stroke-linejoin', 'round');
      icon.classList.add('auth-banner-icon');
      const path1 = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path1.setAttribute('d', 'M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z');
      const line1 = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      line1.setAttribute('x1', '12'); line1.setAttribute('y1', '9');
      line1.setAttribute('x2', '12'); line1.setAttribute('y2', '13');
      const line2 = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      line2.setAttribute('x1', '12'); line2.setAttribute('y1', '17');
      line2.setAttribute('x2', '12.01'); line2.setAttribute('y2', '17');
      icon.appendChild(path1);
      icon.appendChild(line1);
      icon.appendChild(line2);
      left.appendChild(icon);
      const msg = document.createElement('span');
      msg.textContent = 'Authentication needs attention. Token refresh is failing.';
      left.appendChild(msg);
      banner.appendChild(left);
      const btn = document.createElement('button');
      btn.className = 'btn-ghost btn-sm';
      btn.dataset.action = 'dashboard-sign-in';
      btn.textContent = 'Sign In';
      banner.appendChild(btn);
    } else {
      banner.style.display = 'none';
    }
  }

  // Drive cards
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
        if (!drive.online) {
          dot.classList.add('offline');
          dot.setAttribute('aria-label', 'Offline');
        } else if (drive.syncState === 'error') {
          dot.classList.add('error');
          dot.setAttribute('aria-label', 'Error');
        } else if (drive.syncState === 'syncing') {
          dot.classList.add('syncing');
          dot.setAttribute('aria-label', 'Syncing');
        } else {
          dot.classList.add('ok');
          dot.setAttribute('aria-label', 'Online, up to date');
        }
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
        lastSync.textContent = 'Last: ' + formatRelativeTime(drive.lastSynced);

        card.appendChild(header);
        card.appendChild(status);
        card.appendChild(lastSync);
        cardsContainer.appendChild(card);
      });
    } else {
      const empty = document.createElement('div');
      empty.className = 'mount-empty';
      empty.textContent = 'No drives mounted';
      cardsContainer.appendChild(empty);
    }
  }

  // Upload queue summary
  const uploadSummary = document.getElementById('upload-summary');
  const uploadDetail = document.getElementById('upload-detail');
  if (uploadSummary) {
    const ds = state.dashboardStatus;
    const agg = aggregateUploadQueue(ds ? ds.drives : []);
    const total = agg.inFlight + agg.queued;
    if (total > 0) {
      uploadSummary.style.display = '';
      uploadSummary.innerHTML = '';
      const arrow = document.createElement('span');
      arrow.className = 'disclosure-arrow' + (state.writebackExpanded ? ' expanded' : '');
      arrow.textContent = '\u25B6';
      uploadSummary.appendChild(arrow);
      const text = document.createElement('span');
      const parts = [];
      if (agg.inFlight > 0) parts.push(agg.inFlight + ' uploading');
      if (agg.queued > 0) parts.push(agg.queued + ' queued');
      text.textContent = parts.join(', ');
      uploadSummary.appendChild(text);
      uploadSummary.dataset.action = 'toggle-writeback-expanded';
      uploadSummary.setAttribute('aria-expanded', state.writebackExpanded ? 'true' : 'false');
    } else {
      uploadSummary.style.display = 'none';
    }

    if (uploadDetail) {
      const cs = state.cacheStats;
      if (state.writebackExpanded && cs && cs.writebackQueue && cs.writebackQueue.length > 0) {
        uploadDetail.style.display = '';
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

  // Activity feed
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

        const tag = document.createElement('span');
        tag.className = 'activity-tag ' + entry.activityType;
        tag.textContent = entry.activityType;

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
        link.textContent = state.activityExpanded ? 'Show less' : 'Show all (' + entries.length + ')';
        showMore.appendChild(link);
        activityList.appendChild(showMore);
      }
    } else {
      const empty = document.createElement('li');
      empty.className = 'activity-empty';
      empty.textContent = 'No recent activity';
      activityList.appendChild(empty);
    }
  }

  // Error log
  const errorsHeading = document.getElementById('errors-heading');
  const errorList = document.getElementById('error-list');
  if (errorsHeading) {
    const count = state.recentErrors ? state.recentErrors.length : 0;
    errorsHeading.textContent = count > 0 ? 'Errors (' + count + ')' : 'Errors';
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
        fileEl.textContent = err.fileName || 'Unknown file';
        const typeEl = document.createElement('span');
        typeEl.className = 'error-type';
        typeEl.textContent = ' \u2013 ' + (err.errorType || 'error');
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
        if (err.actionHint) {
          const hintEl = document.createElement('div');
          hintEl.className = 'error-hint';
          hintEl.textContent = err.actionHint;
          entry.appendChild(hintEl);
        }

        errorList.appendChild(entry);
      });
    } else {
      const empty = document.createElement('div');
      empty.className = 'error-empty';
      empty.textContent = 'No errors';
      errorList.appendChild(empty);
    }
  }

  // Cache & Offline
  const cacheSection = document.getElementById('cache-section');
  if (cacheSection) {
    cacheSection.innerHTML = '';
    const cs = state.cacheStats;

    // Cache bar (always visible)
    const usedBytes = cs ? cs.diskUsedBytes : 0;
    const maxBytes = cs ? cs.diskMaxBytes : 0;
    const pct = maxBytes > 0 ? Math.min((usedBytes / maxBytes) * 100, 100) : 0;
    const barColor = pct >= 90 ? 'red' : (pct >= 70 ? 'amber' : 'green');

    const bar = document.createElement('div');
    bar.className = 'cache-bar';
    bar.setAttribute('role', 'progressbar');
    bar.setAttribute('aria-valuenow', String(usedBytes));
    bar.setAttribute('aria-valuemin', '0');
    bar.setAttribute('aria-valuemax', String(maxBytes));
    bar.setAttribute('aria-label', 'Cache disk usage');
    const fill = document.createElement('div');
    fill.className = 'cache-bar-fill ' + barColor;
    fill.style.width = pct + '%';
    bar.appendChild(fill);
    cacheSection.appendChild(bar);

    const text = document.createElement('div');
    text.className = 'cache-text';
    text.textContent = formatBytes(usedBytes) + ' / ' + formatBytes(maxBytes);
    cacheSection.appendChild(text);

    // Pin health summary
    const pins = cs ? cs.pinnedItems : [];
    if (pins && pins.length > 0) {
      const summary = document.createElement('div');
      summary.className = 'pin-summary';
      const counts = { downloaded: 0, partial: 0, stale: 0 };
      pins.forEach(function(p) { counts[p.status] = (counts[p.status] || 0) + 1; });
      const parts = [pins.length + ' pins'];
      const statusParts = [];
      if (counts.downloaded > 0) statusParts.push(counts.downloaded + ' Downloaded');
      if (counts.partial > 0) statusParts.push(counts.partial + ' Partial');
      if (counts.stale > 0) statusParts.push(counts.stale + ' Stale');
      summary.textContent = parts[0] + ' \u00B7 ' + statusParts.join(', ');
      cacheSection.appendChild(summary);
    } else {
      const empty = document.createElement('div');
      empty.className = 'pin-summary-empty';
      empty.textContent = 'No offline pins';
      cacheSection.appendChild(empty);
    }
  }
}

function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderHandlers();
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
      offlineTtlSecs: parseInt(document.getElementById('offline-ttl').value) || null,
      offlineMaxFolderSize: document.getElementById('offline-max-size').value || null,
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
      // Derive mount point from settings root dir and library name
      const siteParts = state.settings.primarySiteName || 'SharePoint';
      siteName = siteParts;
      libraryName = driveName;
      mountPoint = mountRoot + '/' + siteName + '/' + libraryName;
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
    showStatus(driveName + ' enabled', 'success');
    const mounts = await invoke('list_mounts');
    setState({ mounts: mounts });
  } catch (e) {
    showStatus(formatError(e), 'error');
    // Re-fetch to revert toggle visual state
    try {
      const mounts = await invoke('list_mounts');
      setState({ mounts: mounts });
    } catch (_) { render(); }
  }
}

async function toggleLibraryOff(mountId, driveName) {
  try {
    await invoke('remove_mount', { id: mountId });
    showStatus(driveName + ' disabled', 'success');
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
// Data refresh helpers
// ---------------------------------------------------------------------------

async function refreshDashboardData() {
  try {
    const [dashboardStatus, cacheStats] = await Promise.all([
      invoke('get_dashboard_status'),
      invoke('get_cache_stats'),
    ]);
    Object.assign(state, { dashboardStatus, cacheStats });
    render();
  } catch (_) { /* silent — dashboard data is best-effort */ }
}

async function refreshOfflineData() {
  try {
    const [offlinePins, cacheStats] = await Promise.all([
      invoke('list_offline_pins'),
      invoke('get_cache_stats'),
    ]);
    Object.assign(state, { offlinePins, cacheStats });
    render();
  } catch (_) { /* silent */ }
}

function refreshPanelData(panel) {
  if (panel === 'dashboard') refreshDashboardData();
  else if (panel === 'offline') refreshOfflineData();
  else if (panel === 'mounts') loadLibraries();
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

async function init() {
  try {
    const [settings, mounts, handlers, offlinePins, dashboardStatus, recentErrors, recentActivity, cacheStats] = await Promise.all([
      invoke('get_settings'),
      invoke('list_mounts'),
      invoke('get_file_handlers'),
      invoke('list_offline_pins'),
      invoke('get_dashboard_status'),
      invoke('get_recent_errors'),
      invoke('get_activity_feed'),
      invoke('get_cache_stats'),
    ]);
    recentActivity.reverse();
    recentErrors.reverse();
    setState({ settings, mounts, handlers, offlinePins, dashboardStatus, recentErrors, recentActivity, cacheStats });
    document.title = settings.app_name + ' Settings';
    document.getElementById('app-version').textContent = settings.app_version;
    // Load libraries in background (non-blocking for initial render)
    loadLibraries();
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
    item.addEventListener('click', () => {
      const panel = item.dataset.panel;
      setState({ activePanel: panel });
      refreshPanelData(panel);
    });
    item.addEventListener('keydown', handleNavKeydown);
  });

  // Auto-save listeners
  ['auto-start', 'notifications', 'explorer-nav-pane', 'sync-interval', 'log-level', 'offline-ttl'].forEach(id =>
    document.getElementById(id).addEventListener('change', saveSettings));
  ['cache-dir', 'cache-max-size', 'metadata-ttl', 'offline-max-size'].forEach(id =>
    document.getElementById(id).addEventListener('input', debouncedSave));

  // Static buttons (direct listeners — not delegated)
  document.getElementById('sign-out-btn').addEventListener('click', signOut);
  document.getElementById('btn-clear-cache').addEventListener('click', clearCache);
  document.getElementById('btn-redetect').addEventListener('click', redetectHandlers);
  document.getElementById('btn-set-default').addEventListener('click', async () => {
    try {
      await invoke('prompt_set_default_handler');
      showStatus('Default Apps settings opened', 'success');
    } catch (e) {
      showStatus(formatError(e), 'error');
    }
  });

  // Delegation for dynamically rendered mount and handler rows
  document.querySelector('.main-content').addEventListener('click', async (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    const action = target.dataset.action;
    if (action === 'override-handler') showOverrideInput(target);
    else if (action === 'set-override') await setOverride(target);
    else if (action === 'clear-override') await clearOverride(target);
    else if (action === 'remove-pin') await removeOfflinePin(target.dataset.driveId, target.dataset.itemId, target.dataset.name);
    else if (action === 'toggle-activity-expanded') {
      setState({ activityExpanded: !state.activityExpanded });
    }
    else if (action === 'toggle-writeback-expanded') {
      setState({ writebackExpanded: !state.writebackExpanded });
    }
    else if (action === 'dashboard-sign-in') {
      try {
        await invoke('sign_out');
        showStatus('Please sign in again', 'info');
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

  // Backend-triggered refresh (tray icon re-opens window)
  listen('refresh-settings', async () => {
    try {
      const [settings, mounts, offlinePins, dashboardStatus, cacheStats] = await Promise.all([
        invoke('get_settings'),
        invoke('list_mounts'),
        invoke('list_offline_pins'),
        invoke('get_dashboard_status'),
        invoke('get_cache_stats'),
      ]);
      setState({ settings, mounts, offlinePins, dashboardStatus, cacheStats });
    } catch (_) { /* silent */ }
  });

  // Real-time dashboard events
  listen('obs-event', function(event) {
    const p = event.payload;
    const ds = state.dashboardStatus;
    if (!ds) return;

    switch (p.type) {
      case 'syncStateChanged': {
        const drive = ds.drives.find(function(d) { return d.driveId === p.driveId; });
        if (drive) {
          const wasSyncing = drive.syncState === 'syncing';
          drive.syncState = p.state;
          if (wasSyncing && p.state !== 'syncing') {
            drive.lastSynced = new Date().toISOString();
            refreshDashboardData();
          }
        }
        break;
      }
      case 'onlineStateChanged': {
        const drive = ds.drives.find(function(d) { return d.driveId === p.driveId; });
        if (drive) drive.online = p.online;
        break;
      }
      case 'authStateChanged': {
        ds.authDegraded = p.degraded;
        break;
      }
      case 'error': {
        state.recentErrors.unshift({
          driveId: p.driveId,
          fileName: p.fileName,
          remotePath: p.remotePath,
          errorType: p.errorType,
          message: p.message,
          actionHint: p.actionHint,
          timestamp: p.timestamp,
        });
        if (state.recentErrors.length > 100) state.recentErrors.length = 100;
        break;
      }
      case 'activity': {
        state.recentActivity.unshift({
          driveId: p.driveId,
          filePath: p.filePath,
          activityType: p.activityType,
          timestamp: p.timestamp,
        });
        if (state.recentActivity.length > 500) state.recentActivity.length = 500;
        break;
      }
    }
    scheduleRender();
  });

  // Periodic data refresh: re-fetch dashboard/offline data every 30 seconds
  setInterval(function() {
    refreshPanelData(state.activePanel);
  }, 30000);
}

init();
