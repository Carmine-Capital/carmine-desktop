const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let signingIn = false;
let activeListeners = [];
let selectedSite = null;

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
  const mounts = await invoke('list_mounts');
  if (mounts.length > 0) {
    renderMountList(mounts);
    showStep('step-done');
  } else {
    showStep('step-source');
  }
}

function cleanupListeners() {
  activeListeners.forEach(fn => { try { fn(); } catch (_) {} });
  activeListeners = [];
}

// -- step-source: source selection --

async function selectSource(type) {
  if (type === 'sharepoint') {
    document.getElementById('sp-search').value = '';
    document.getElementById('sp-error').style.display = 'none';
    document.getElementById('sp-error').textContent = '';
    document.getElementById('sp-sites').innerHTML = '';
    document.getElementById('sp-libraries').style.display = 'none';
    document.getElementById('sp-spinner').style.display = 'none';
    showStep('step-sharepoint');
    return;
  }

  // OneDrive path
  const errEl = document.getElementById('source-error');
  errEl.style.display = 'none';
  errEl.textContent = '';

  let mounts;
  try {
    mounts = await invoke('list_mounts');
  } catch (e) {
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
    return;
  }

  const driveMount = mounts.find(m => m.mount_type === 'drive');
  if (!driveMount || !driveMount.drive_id) {
    errEl.textContent = 'OneDrive is not yet available \u2014 please wait a moment and try again';
    errEl.style.display = 'block';
    return;
  }

  const driveCount = mounts.filter(m => m.mount_type === 'drive').length;
  const mountPoint = '~/Cloud/OneDrive ' + (driveCount + 1) + '/';

  try {
    await invoke('add_mount', {
      mountType: 'drive',
      driveId: driveMount.drive_id,
      mountPoint,
    });
    await refreshAndFinish();
  } catch (e) {
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
  }
}

// -- step-sharepoint: site search --

async function searchSites() {
  const query = document.getElementById('sp-search').value.trim();
  const spinner = document.getElementById('sp-spinner');
  const errEl = document.getElementById('sp-error');
  const sitesEl = document.getElementById('sp-sites');

  errEl.style.display = 'none';
  errEl.textContent = '';
  sitesEl.innerHTML = '';
  document.getElementById('sp-libraries').style.display = 'none';
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
    row.onclick = () => selectSite(site);
    sitesEl.appendChild(row);
  });
}

// -- step-sharepoint: site selection and library listing --

async function selectSite(site) {
  selectedSite = site;
  const spinner = document.getElementById('sp-spinner');
  const errEl = document.getElementById('sp-error');
  const libList = document.getElementById('sp-lib-list');

  spinner.style.display = 'inline-block';
  errEl.style.display = 'none';
  errEl.textContent = '';
  libList.innerHTML = '';

  let libraries;
  try {
    libraries = await invoke('list_drives', { siteId: site.id });
  } catch (e) {
    spinner.style.display = 'none';
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
    return;
  }
  spinner.style.display = 'none';

  if (libraries.length === 1) {
    await confirmMount(site, libraries[0]);
    return;
  }

  libraries.forEach(lib => {
    const row = document.createElement('div');
    row.className = 'sp-result-row';
    row.textContent = lib.name;
    row.onclick = () => confirmMount(selectedSite, lib);
    libList.appendChild(row);
  });
  document.getElementById('sp-libraries').style.display = 'block';
}

function showSitesBack() {
  document.getElementById('sp-libraries').style.display = 'none';
}

// -- step-sharepoint: mount confirmation --

async function confirmMount(site, library) {
  const spinner = document.getElementById('sp-spinner');
  const errEl = document.getElementById('sp-error');
  spinner.style.display = 'inline-block';

  const mountPoint = '~/Cloud/' + site.display_name + ' - ' + library.name + '/';
  try {
    await invoke('add_mount', {
      mountType: 'sharepoint',
      mountPoint,
      driveId: library.id,
      siteId: site.id,
      siteName: site.display_name,
      libraryName: library.name,
    });
  } catch (e) {
    spinner.style.display = 'none';
    errEl.textContent = e.toString();
    errEl.style.display = 'block';
    return;
  }
  spinner.style.display = 'none';
  await refreshAndFinish();
}

async function refreshAndFinish() {
  const mounts = await invoke('list_mounts');
  renderMountList(mounts);
  showStep('step-done');
}

// -- step-done: mount list rendering --

function renderMountList(mounts) {
  const list = document.getElementById('done-mount-list');
  list.innerHTML = '';
  mounts.forEach(m => {
    const li = document.createElement('li');
    li.className = 'mount-item';
    li.textContent = m.name + ' \u2192 ' + m.mount_point;
    list.appendChild(li);
  });
}

function showStep(id) {
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  document.getElementById(id).classList.add('active');
}

async function init() {
  try {
    const settings = await invoke('get_settings');
    document.getElementById('app-title').textContent = settings.app_name;
    document.title = settings.app_name + ' Setup';

    const mounts = await invoke('list_mounts');
    if (mounts.length > 0) {
      const list = document.getElementById('preconfigured-mounts');
      list.style.display = 'block';
      mounts.forEach(m => {
        const div = document.createElement('div');
        div.className = 'mount-item';
        div.textContent = m.name + ' \u2192 ' + m.mount_point;
        list.appendChild(div);
      });
    }
  } catch (e) { console.error(e); }

  document.getElementById('sign-in-btn').addEventListener('click', startSignIn);
  document.getElementById('copy-btn').addEventListener('click', copyAuthUrl);
  document.getElementById('cancel-btn').addEventListener('click', cancelSignIn);
  document.getElementById('source-drive-btn').addEventListener('click', () => selectSource('drive'));
  document.getElementById('source-sp-btn').addEventListener('click', () => selectSource('sharepoint'));
  document.getElementById('sp-back-btn').addEventListener('click', () => showStep('step-source'));
  document.getElementById('sp-search-btn').addEventListener('click', searchSites);
  document.getElementById('sp-back-sites-btn').addEventListener('click', showSitesBack);
  document.getElementById('wizard-close-btn').addEventListener('click', () => {
    window.__TAURI__.window.getCurrentWindow().close();
  });

  const spSearch = document.getElementById('sp-search');
  if (spSearch) {
    spSearch.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') searchSites();
    });
  }
}
init();
