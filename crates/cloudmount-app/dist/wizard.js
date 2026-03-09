const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let signingIn = false;
let activeListeners = [];
let onedriveDriveId = null;
let sourcesSpSearchTimer = null;
let sourcesSelectedSite = null;
let addMountMode = false;
let selectedLibraries = new Map(); // driveId → { site, library }

// -- sign-in flow --

async function startSignIn() {
  if (signingIn) return;
  signingIn = true;
  document.getElementById('auth-error').style.display = 'none';
  showStep('step-signing-in');

  // Register event listeners before invoking so we don't miss events.
  activeListeners.push(await listen('auth-complete', async () => {
    if (!signingIn) return;
    signingIn = false;
    cleanupListeners();
    await onSignInComplete();
  }));
  activeListeners.push(await listen('auth-error', (event) => {
    if (!signingIn) return;
    signingIn = false;
    cleanupListeners();
    const errEl = document.getElementById('auth-error');
    errEl.textContent = 'Sign-in failed: ' + (event.payload || 'unknown error');
    errEl.style.display = 'block';
  }));

  try {
    const authUrl = await invoke('start_sign_in');
    document.getElementById('auth-url').value = authUrl;
  } catch (e) {
    signingIn = false;
    cleanupListeners();
    console.error('start_sign_in failed:', e);
    showStep('step-welcome');
  }
}

async function cancelSignIn() {
  signingIn = false;
  try { await invoke('cancel_sign_in'); } catch (_) {}
  cleanupListeners();
  showStep('step-welcome');
  document.getElementById('auth-url').value = '';
  const errEl = document.getElementById('auth-error');
  errEl.style.display = 'none';
  errEl.textContent = '';
}

async function copyAuthUrl() {
  const url = document.getElementById('auth-url').value;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    const btn = document.getElementById('copy-btn');
    btn.textContent = 'Copied!';
    setTimeout(() => { btn.textContent = 'Copy URL'; }, 2000);
  } catch (e) {
    console.error('clipboard write failed:', e);
  }
}

async function onSignInComplete() {
  showStep('step-sources');
  await loadSources();
}

async function goToAddMount() {
  addMountMode = true;
  await onSignInComplete();
}

function cleanupListeners() {
  activeListeners.forEach(fn => { try { fn(); } catch (_) {} });
  activeListeners = [];
}

// -- step-sources: loading drive info and followed sites --

async function loadSources() {
  document.getElementById('sources-loading').style.display = 'block';
  document.getElementById('sources-onedrive-section').style.display = 'none';
  document.getElementById('sources-sp-section').style.display = 'none';
  document.getElementById('sources-error').style.display = 'none';

  const [driveResult, sitesResult] = await Promise.allSettled([
    invoke('get_drive_info'),
    invoke('get_followed_sites'),
  ]);

  document.getElementById('sources-loading').style.display = 'none';

  if (driveResult.status === 'fulfilled') {
    const drive = driveResult.value;
    onedriveDriveId = drive.id;
    document.getElementById('onedrive-drive-name').textContent = drive.name || 'OneDrive';
    document.getElementById('sources-onedrive-section').style.display = 'block';
  }

  if (addMountMode) {
    document.getElementById('sources-onedrive-section').style.display = 'none';
    const btn = document.getElementById('get-started-btn');
    btn.textContent = 'Close';
    btn.disabled = false;
  }

  if (sitesResult.status === 'fulfilled') {
    document.getElementById('sources-sp-section').style.display = 'block';
    renderFollowedSites(sitesResult.value);
  }

  if (driveResult.status === 'rejected' && sitesResult.status === 'rejected') {
    const errEl = document.getElementById('sources-error');
    errEl.textContent = 'Could not load account data \u2014 please try signing in again.';
    errEl.style.display = 'block';
  }

  updateGetStartedBtn();
}

function renderFollowedSites(sites) {
  const sitesEl = document.getElementById('sources-sp-sites');
  sitesEl.innerHTML = '';
  sites.forEach(site => {
    const row = document.createElement('div');
    row.className = 'sp-result-row';
    const name = document.createElement('div');
    name.textContent = site.display_name;
    const url = document.createElement('div');
    url.className = 'sp-result-url';
    url.textContent = site.web_url;
    row.appendChild(name);
    row.appendChild(url);
    row.onclick = () => selectSiteInSources(site);
    sitesEl.appendChild(row);
  });
}

// -- step-sources: SharePoint search (debounced) --

function onSourcesSpSearchInput() {
  clearTimeout(sourcesSpSearchTimer);
  const query = document.getElementById('sources-sp-search').value.trim();
  if (!query) {
    // Restore followed sites on empty query
    loadSources();
    return;
  }
  sourcesSpSearchTimer = setTimeout(() => searchSitesInSources(query), 300);
}

async function searchSitesInSources(query) {
  const spinner = document.getElementById('sources-sp-spinner');
  const errEl = document.getElementById('sources-sp-error');
  const sitesEl = document.getElementById('sources-sp-sites');

  errEl.style.display = 'none';
  errEl.textContent = '';
  sitesEl.innerHTML = '';
  document.getElementById('sources-sp-libraries').style.display = 'none';
  spinner.style.display = 'inline-block';

  let sites;
  try {
    sites = await invoke('search_sites', { query });
  } catch (e) {
    spinner.style.display = 'none';
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
    return;
  }
  spinner.style.display = 'none';

  if (sites.length === 0) {
    errEl.textContent = 'No sites found \u2014 try a different search term';
    errEl.style.display = 'block';
    return;
  }

  renderFollowedSites(sites);
}

// -- step-sources: site and library selection --

async function selectSiteInSources(site) {
  sourcesSelectedSite = site;
  selectedLibraries.clear();
  const spinner = document.getElementById('sources-sp-spinner');
  const errEl = document.getElementById('sources-sp-error');
  const libList = document.getElementById('sources-sp-lib-list');

  spinner.style.display = 'inline-block';
  errEl.style.display = 'none';
  errEl.textContent = '';
  libList.innerHTML = '';

  let libraries, mounts;
  try {
    [libraries, mounts] = await Promise.all([
      invoke('list_drives', { siteId: site.id }),
      invoke('list_mounts'),
    ]);
  } catch (e) {
    spinner.style.display = 'none';
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
    return;
  }
  spinner.style.display = 'none';

  const mountedDriveIds = new Set(mounts.map(m => m.drive_id).filter(Boolean));

  libraries.forEach(lib => {
    const isMounted = mountedDriveIds.has(lib.id);
    const row = document.createElement('div');
    row.className = 'sp-lib-row' + (isMounted ? ' mounted' : '');
    row.dataset.driveId = lib.id;

    const check = document.createElement('div');
    check.className = 'lib-check';
    check.textContent = '\u2713';

    const info = document.createElement('div');
    info.className = 'lib-info';
    const name = document.createElement('div');
    name.className = 'lib-name';
    name.textContent = lib.name;
    info.appendChild(name);

    if (isMounted) {
      const badge = document.createElement('div');
      badge.className = 'lib-badge';
      badge.textContent = 'Already added';
      info.appendChild(badge);
    }

    row.appendChild(check);
    row.appendChild(info);

    if (!isMounted) {
      row.addEventListener('click', () => {
        const driveId = lib.id;
        if (selectedLibraries.has(driveId)) {
          selectedLibraries.delete(driveId);
          row.classList.remove('selected');
        } else {
          selectedLibraries.set(driveId, { site, library: lib });
          row.classList.add('selected');
        }
        updateAddSelectedBtn();
      });
    }

    libList.appendChild(row);
  });

  document.getElementById('sources-sp-libraries').style.display = 'block';
  updateAddSelectedBtn();
}

function updateAddSelectedBtn() {
  const btn = document.getElementById('add-selected-btn');
  const count = selectedLibraries.size;
  if (count > 0) {
    btn.style.display = 'block';
    btn.disabled = false;
    btn.textContent = 'Add selected (' + count + ')';
  } else {
    btn.style.display = 'none';
    btn.disabled = true;
  }
}

async function confirmSelectedLibraries() {
  if (selectedLibraries.size === 0) return;
  const errEl = document.getElementById('sources-sp-error');
  const addBtn = document.getElementById('add-selected-btn');

  addBtn.disabled = true;
  errEl.style.display = 'none';

  const entries = Array.from(selectedLibraries.entries());
  const total = entries.length;
  const errors = [];
  const succeeded = [];

  for (let i = 0; i < entries.length; i++) {
    const [driveId, { site, library }] = entries[i];
    addBtn.textContent = 'Adding ' + (i + 1) + ' of ' + total + '\u2026';

    const mountPoint = '~/Cloud/' + site.display_name + ' - ' + library.name + '/';
    try {
      const mountId = await invoke('add_mount', {
        mountType: 'sharepoint',
        mountPoint,
        driveId: library.id,
        siteId: site.id,
        siteName: site.display_name,
        libraryName: library.name,
      });
      succeeded.push(driveId);
      addSourceEntry(library.name + ' (' + site.display_name + ')', mountId);

      // Transition row to mounted state
      const row = document.querySelector('.sp-lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
      if (row) {
        row.classList.remove('selected');
        row.classList.add('mounted');
        row.replaceWith(row.cloneNode(true)); // remove click listener
        const badge = document.createElement('div');
        badge.className = 'lib-badge';
        badge.textContent = 'Already added';
        const mountedRow = document.querySelector('.sp-lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
        const infoEl = mountedRow.querySelector('.lib-info');
        if (infoEl && !infoEl.querySelector('.lib-badge')) {
          infoEl.appendChild(badge);
        }
      }
    } catch (e) {
      errors.push(library.name + ': ' + e.toString());
    }
  }

  // Clear only succeeded items from selection; keep failed for retry
  for (const driveId of succeeded) {
    selectedLibraries.delete(driveId);
  }

  updateAddSelectedBtn();
  updateGetStartedBtn();

  if (errors.length === 0) {
    showStatus(total === 1
      ? 'Library added successfully'
      : total + ' libraries added successfully', 'success');
  } else if (errors.length === total) {
    showStatus('Failed to add libraries \u2014 check your connection', 'error');
  } else {
    errEl.textContent = 'Some libraries failed: ' + errors.join('; ');
    errEl.style.display = 'block';
    showStatus(succeeded.length + ' added, ' + errors.length + ' failed', 'info');
  }
}

function addSourceEntry(label, mountId) {
  const section = document.getElementById('sources-added-section');
  const list = document.getElementById('sources-added-list');

  const row = document.createElement('div');
  row.className = 'added-source-row';

  const nameEl = document.createElement('div');
  nameEl.className = 'added-source-name';
  nameEl.textContent = label;

  const removeBtn = document.createElement('button');
  removeBtn.className = 'btn-remove';
  removeBtn.textContent = 'Remove';
  removeBtn.onclick = async () => {
    if (mountId) {
      try { await invoke('remove_mount', { id: mountId }); } catch (_) {}
    }
    list.removeChild(row);
    if (list.children.length === 0) section.style.display = 'none';
    updateGetStartedBtn();
  };

  row.appendChild(nameEl);
  row.appendChild(removeBtn);
  list.appendChild(row);
  section.style.display = 'block';
}

// -- step-sources: Get started --

function updateGetStartedBtn() {
  if (addMountMode) return;
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    document.getElementById('sources-onedrive-section').style.display !== 'none';
  const hasAdded = document.getElementById('sources-added-list').children.length > 0;
  document.getElementById('get-started-btn').disabled = !(onedriveChecked || hasAdded);
}

async function getStarted() {
  const errEl = document.getElementById('sources-error');
  errEl.style.display = 'none';

  const onedriveSection = document.getElementById('sources-onedrive-section');
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    onedriveSection.style.display !== 'none';

  if (onedriveChecked && onedriveDriveId) {
    try {
      await invoke('add_mount', {
        mountType: 'drive',
        driveId: onedriveDriveId,
        mountPoint: '~/Cloud/OneDrive',
      });
    } catch (e) {
      errEl.textContent = e.toString();
      errEl.style.display = 'block';
      return;
    }
  }

  try {
    await invoke('complete_wizard');
  } catch (_) {}

  const mounts = await invoke('list_mounts');
  const list = document.getElementById('done-mount-list');
  list.innerHTML = '';
  mounts.forEach(m => {
    const li = document.createElement('li');
    li.className = 'mount-item';
    li.textContent = m.name + ' \u2192 ' + m.mount_point;
    list.appendChild(li);
  });
  showStep('step-success');
}

// -- utilities --

function showStep(id) {
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  document.getElementById(id).classList.add('active');
}

async function init() {
  document.getElementById('sign-in-btn').addEventListener('click', startSignIn);
  document.getElementById('copy-btn').addEventListener('click', copyAuthUrl);
  document.getElementById('cancel-btn').addEventListener('click', cancelSignIn);
  document.getElementById('sources-sp-back-sites').addEventListener('click', () => {
    document.getElementById('sources-sp-libraries').style.display = 'none';
    selectedLibraries.clear();
    updateAddSelectedBtn();
  });
  document.getElementById('add-selected-btn').addEventListener('click', confirmSelectedLibraries);
  document.getElementById('sources-sp-search').addEventListener('input', onSourcesSpSearchInput);
  document.getElementById('onedrive-check').addEventListener('change', updateGetStartedBtn);
  document.getElementById('get-started-btn').addEventListener('click', () => {
    if (addMountMode) {
      window.__TAURI__.window.getCurrentWindow().close();
    } else {
      getStarted();
    }
  });
  document.getElementById('wizard-close-btn').addEventListener('click', () => {
    window.__TAURI__.window.getCurrentWindow().close();
  });

  const authenticated = await invoke('is_authenticated');
  if (authenticated) {
    await goToAddMount();
  }
}
init();
