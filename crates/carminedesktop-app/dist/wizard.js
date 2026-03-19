const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const state = {
  step: 'step-welcome',
  signingIn: false,
  addMountMode: false,
  onedriveDriveId: null,
  defaultMountRoot: '~/Cloud',
  followedSites: [],
  selectedSite: null,
  libraries: [],
  selectedLibraries: new Map(),
  addedSources: [],
  authUnlisteners: [],
  finalMounts: [],
};

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

function sanitizePath(name) {
  return name.replace(/[/\\:*?"<>|]/g, '_').trim() || '_';
}

const AUTH_TIMEOUT_SECS = 120;
let countdownTimer = null;

function startCountdown() {
  stopCountdown();
  let remaining = AUTH_TIMEOUT_SECS;
  const el = document.getElementById('auth-countdown');
  el.textContent = 'Time remaining: ' + remaining + 's';
  el.className = 'auth-countdown';

  countdownTimer = setInterval(() => {
    remaining--;
    if (remaining <= 0) {
      stopCountdown();
      el.textContent = '';
      return;
    }
    el.textContent = 'Time remaining: ' + remaining + 's';
    if (remaining <= 30) {
      el.className = 'auth-countdown warning';
    }
  }, 1000);
}

function stopCountdown() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  const el = document.getElementById('auth-countdown');
  if (el) {
    el.textContent = '';
    el.className = 'auth-countdown';
  }
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

const _stepTitles = {
  'step-welcome': 'Carmine Desktop Setup',
  'step-signing-in': 'Sign In \u2014 Carmine Desktop Setup',
  'step-sources': 'Add Sources \u2014 Carmine Desktop Setup',
  'step-success': 'All Set \u2014 Carmine Desktop Setup',
};

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
  const footer = document.getElementById('wizard-footer');
  if (footer) {
    if (currentStep === 3) {
      const count = state.addedSources.length;
      footer.textContent = count > 0 ? count + ' sources added' : '';
      footer.style.display = count > 0 ? '' : 'none';
    } else {
      footer.textContent = '';
      footer.style.display = 'none';
    }
  }
}

function goToStep(stepId) {
  state.step = stepId;
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  document.getElementById(stepId).classList.add('active');
  updateStepper(stepId);
  if (_stepTitles[stepId]) document.title = _stepTitles[stepId];
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

function cleanupAuthListeners() {
  state.authUnlisteners.forEach(fn => { try { fn(); } catch (_) {} });
  state.authUnlisteners = [];
}

async function startSignIn() {
  if (state.signingIn) return;

  try {
    const fuseOk = await invoke('check_fuse_available');
    if (!fuseOk) {
      showStatus('FUSE is not installed. Install libfuse3 (Linux) or macFUSE (macOS) to use Carmine Desktop.', 'error');
      return;
    }
  } catch (e) {
    console.warn('FUSE check failed, proceeding:', e);
  }

  state.signingIn = true;
  document.getElementById('auth-error').style.display = 'none';
  goToStep('step-signing-in');
  startCountdown();

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
    const errEl = document.getElementById('auth-error');
    errEl.textContent = 'Sign-in failed: ' + (event.payload || 'unknown error');
    errEl.style.display = 'block';
  }));

  try {
    const authUrl = await invoke('start_sign_in');
    document.getElementById('auth-url').value = authUrl;
  } catch (e) {
    state.signingIn = false;
    stopCountdown();
    cleanupAuthListeners();
    console.error('start_sign_in failed:', e);
    showStatus('Sign-in failed', 'error');
    goToStep('step-welcome');
  }
}

async function cancelSignIn() {
  state.signingIn = false;
  stopCountdown();
  try { await invoke('cancel_sign_in'); } catch (e) { console.warn('cancel failed:', e); }
  cleanupAuthListeners();
  goToStep('step-welcome');
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
    showStatus('Could not copy URL', 'error');
  }
}

async function onSignInComplete() {
  goToStep('step-sources');
  await loadSources();
}

async function goToAddMount() {
  state.addMountMode = true;
  await onSignInComplete();
}

// ---------------------------------------------------------------------------
// Sources — loading
// ---------------------------------------------------------------------------

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
    state.onedriveDriveId = drive.id;
    document.getElementById('onedrive-drive-name').textContent = drive.name || 'OneDrive';
    document.getElementById('onedrive-mount-path').textContent = state.defaultMountRoot + '/OneDrive';
    document.getElementById('sources-onedrive-section').style.display = 'block';
  }

  if (state.addMountMode) {
    document.getElementById('sources-onedrive-section').style.display = 'none';
    const btn = document.getElementById('get-started-btn');
    btn.textContent = 'Close';
    btn.disabled = false;
  }

  if (sitesResult.status === 'fulfilled') {
    state.followedSites = sitesResult.value;
    document.getElementById('sources-sp-section').style.display = 'block';
    renderFollowedSites(sitesResult.value);
  }

  if (driveResult.status === 'rejected' && sitesResult.status === 'rejected') {
    const errEl = document.getElementById('sources-error');
    errEl.textContent = 'Could not load account data \u2014 please try signing in again.';
    errEl.style.display = 'block';
  } else if (driveResult.status === 'rejected') {
    showStatus('OneDrive info unavailable \u2014 SharePoint sites loaded', 'info');
  } else if (sitesResult.status === 'rejected') {
    showStatus('SharePoint sites unavailable \u2014 OneDrive loaded', 'info');
  }

  updateGetStartedBtn();
}

// ---------------------------------------------------------------------------
// Sources — rendering
// ---------------------------------------------------------------------------

let _displayedSites = [];

function renderFollowedSites(sites) {
  _displayedSites = sites;
  const sitesEl = document.getElementById('sources-sp-sites');
  sitesEl.innerHTML = '';
  if (sites.length === 0) {
    const hint = document.createElement('p');
    hint.className = 'sp-empty-hint';
    hint.textContent = 'No followed sites yet. Follow sites in SharePoint or use the search box above to find them.';
    sitesEl.appendChild(hint);
    return;
  }
  sites.forEach(site => {
    const row = document.createElement('div');
    row.className = 'sp-result-row';
    row.dataset.action = 'select-site';
    row.dataset.siteId = site.id;
    row.setAttribute('role', 'button');
    row.setAttribute('tabindex', '0');
    const name = document.createElement('div');
    name.textContent = site.display_name;
    const url = document.createElement('div');
    url.className = 'sp-result-url';
    url.textContent = site.web_url;
    row.appendChild(name);
    row.appendChild(url);
    sitesEl.appendChild(row);
  });
}

function renderAddedSources() {
  const section = document.getElementById('sources-added-section');
  const list = document.getElementById('sources-added-list');
  list.innerHTML = '';
  state.addedSources.forEach(s => {
    const row = document.createElement('div');
    row.className = 'added-source-row';
    const nameEl = document.createElement('div');
    nameEl.className = 'added-source-name';
    nameEl.textContent = s.label;
    const btn = document.createElement('button');
    btn.className = 'btn-icon btn-icon-danger';
    btn.dataset.action = 'remove-source';
    btn.dataset.mountId = s.mountId;
    btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>';
    row.appendChild(nameEl);
    row.appendChild(btn);
    list.appendChild(row);
  });
  section.style.display = state.addedSources.length > 0 ? 'block' : 'none';
}

function addSourceEntry(label, mountId) {
  state.addedSources.push({ label, mountId });
  renderAddedSources();
}

// ---------------------------------------------------------------------------
// Sources — search
// ---------------------------------------------------------------------------

let sourcesSpSearchTimer = null;

function onSourcesSpSearchInput() {
  clearTimeout(sourcesSpSearchTimer);
  const query = document.getElementById('sources-sp-search').value.trim();
  if (!query) {
    renderFollowedSites(state.followedSites);
    document.getElementById('sources-sp-libraries').style.display = 'none';
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
    errEl.textContent = formatError(e);
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

// ---------------------------------------------------------------------------
// Sources — site & library selection
// ---------------------------------------------------------------------------

function selectSiteById(siteId) {
  const site = _displayedSites.find(s => s.id === siteId);
  if (site) selectSiteInSources(site);
}

async function selectSiteInSources(site) {
  state.selectedSite = site;
  if (state.selectedLibraries.size > 0) {
    state.selectedLibraries.clear();
    showStatus('Previous selections cleared', 'info');
  } else {
    state.selectedLibraries.clear();
  }
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
    errEl.textContent = formatError(e);
    errEl.style.display = 'block';
    return;
  }
  spinner.style.display = 'none';

  state.libraries = libraries;
  const mountedDriveIds = new Set(mounts.map(m => m.drive_id).filter(Boolean));

  libraries.forEach(lib => {
    const isMounted = mountedDriveIds.has(lib.id);
    const row = document.createElement('div');
    row.className = 'lib-row' + (isMounted ? ' mounted' : '');
    row.dataset.driveId = lib.id;
    if (!isMounted) {
      row.dataset.action = 'toggle-lib';
      row.setAttribute('role', 'checkbox');
      row.setAttribute('aria-checked', 'false');
      row.setAttribute('tabindex', '0');
    }

    const check = document.createElement('div');
    check.className = 'lib-check';
    check.textContent = '\u2713';
    check.setAttribute('aria-hidden', 'true');

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
    libList.appendChild(row);
  });

  document.getElementById('sources-sp-libraries').style.display = 'block';
  updateAddSelectedBtn();
}

function toggleLibrary(driveId) {
  if (state.selectedLibraries.has(driveId)) {
    state.selectedLibraries.delete(driveId);
  } else {
    const lib = state.libraries.find(l => l.id === driveId);
    if (lib && state.selectedSite) {
      state.selectedLibraries.set(driveId, { site: state.selectedSite, library: lib });
    }
  }
  const row = document.querySelector('.lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
  if (row) {
    const isSelected = state.selectedLibraries.has(driveId);
    row.classList.toggle('selected', isSelected);
    row.setAttribute('aria-checked', isSelected ? 'true' : 'false');
  }
  updateAddSelectedBtn();
}

function updateAddSelectedBtn() {
  const btn = document.getElementById('add-selected-btn');
  const count = state.selectedLibraries.size;
  if (count > 0) {
    btn.style.display = 'block';
    btn.disabled = false;
    btn.textContent = 'Add selected (' + count + ')';
  } else {
    btn.style.display = 'none';
    btn.disabled = true;
  }
}

function backToSites() {
  state.selectedSite = null;
  state.libraries = [];
  state.selectedLibraries.clear();
  document.getElementById('sources-sp-libraries').style.display = 'none';
  updateAddSelectedBtn();
}

async function confirmSelectedLibraries() {
  if (state.selectedLibraries.size === 0) return;
  const errEl = document.getElementById('sources-sp-error');
  const addBtn = document.getElementById('add-selected-btn');

  addBtn.disabled = true;
  errEl.style.display = 'none';

  const entries = Array.from(state.selectedLibraries.entries());
  const total = entries.length;
  const errors = [];
  const succeeded = [];

  for (let i = 0; i < entries.length; i++) {
    const [driveId, { site, library }] = entries[i];
    addBtn.textContent = 'Adding ' + (i + 1) + ' of ' + total + '\u2026';

    const safeSite = sanitizePath(site.display_name);
    const safeLib = sanitizePath(library.name);
    const mountPoint = state.defaultMountRoot + '/' + safeSite + ' - ' + safeLib + '/';
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
      const row = document.querySelector('.lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
      if (row) {
        row.classList.remove('selected');
        row.classList.add('mounted');
        row.removeAttribute('role');
        row.removeAttribute('tabindex');
        delete row.dataset.action;
        const infoEl = row.querySelector('.lib-info');
        if (infoEl && !infoEl.querySelector('.lib-badge')) {
          const badge = document.createElement('div');
          badge.className = 'lib-badge';
          badge.textContent = 'Already added';
          infoEl.appendChild(badge);
        }
      }
    } catch (e) {
      errors.push(library.name + ': ' + formatError(e));
    }
  }

  for (const driveId of succeeded) {
    state.selectedLibraries.delete(driveId);
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

async function removeSource(mountId) {
  const btn = document.querySelector('[data-action="remove-source"][data-mount-id="' + CSS.escape(mountId) + '"]');
  if (btn) btn.disabled = true;
  if (mountId) {
    try {
      await invoke('remove_mount', { id: mountId });
    } catch (e) {
      if (btn) btn.disabled = false;
      showStatus(formatError(e), 'error');
      return;
    }
  }
  state.addedSources = state.addedSources.filter(s => s.mountId !== mountId);
  renderAddedSources();
  updateGetStartedBtn();
  showStatus('Source removed', 'success');
}

// ---------------------------------------------------------------------------
// Get Started / Complete
// ---------------------------------------------------------------------------

function updateGetStartedBtn() {
  if (state.addMountMode) return;
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    document.getElementById('sources-onedrive-section').style.display !== 'none';
  const hasAdded = state.addedSources.length > 0;
  document.getElementById('get-started-btn').disabled = !(onedriveChecked || hasAdded);
  // Update stepper footer
  const footer = document.getElementById('wizard-footer');
  if (footer) {
    const count = state.addedSources.length;
    footer.textContent = count > 0 ? count + ' sources added' : '';
    footer.style.display = count > 0 ? '' : 'none';
  }
}

function handleGetStarted() {
  if (state.addMountMode) {
    window.__TAURI__.window.getCurrentWindow().close();
  } else {
    getStarted();
  }
}

async function getStarted() {
  const errEl = document.getElementById('sources-error');
  errEl.style.display = 'none';

  const btn = document.getElementById('get-started-btn');
  const origLabel = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Setting up\u2026';

  const onedriveSection = document.getElementById('sources-onedrive-section');
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    onedriveSection.style.display !== 'none';

  if (onedriveChecked && state.onedriveDriveId) {
    try {
      await invoke('add_mount', {
        mountType: 'drive',
        driveId: state.onedriveDriveId,
        mountPoint: state.defaultMountRoot + '/OneDrive',
      });
    } catch (e) {
      errEl.textContent = formatError(e);
      errEl.style.display = 'block';
      btn.disabled = false;
      btn.textContent = origLabel;
      return;
    }
  }

  try {
    await invoke('complete_wizard');
  } catch (e) {
    showStatus('Failed to complete setup', 'error');
    btn.disabled = false;
    btn.textContent = origLabel;
    return;
  }

  try {
    const mounts = await invoke('list_mounts');
    state.finalMounts = mounts;
    const list = document.getElementById('done-mount-list');
    list.innerHTML = '';
    mounts.forEach(m => {
      const li = document.createElement('li');
      li.className = 'mount-item';
      li.textContent = m.name + ' \u2192 ' + m.mount_point;
      list.appendChild(li);
    });
  } catch (e) {
    console.error('list_mounts failed:', e);
  }
  goToStep('step-success');
}

// ---------------------------------------------------------------------------
// Switch Account
// ---------------------------------------------------------------------------

async function switchAccount() {
  const btn = document.getElementById('switch-account-btn');
  btn.disabled = true;
  btn.textContent = 'Signing out\u2026';
  try {
    await invoke('sign_out');
  } catch (e) {
    console.warn('sign_out during switch failed:', e);
  }
  btn.disabled = false;
  btn.textContent = 'Sign in with a different account';
  state.addMountMode = false;
  state.onedriveDriveId = null;
  state.followedSites = [];
  state.selectedSite = null;
  state.libraries = [];
  state.selectedLibraries.clear();
  state.addedSources = [];
  goToStep('step-welcome');
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

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
  // Keyboard support for delegated rows
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
