const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// État
// ---------------------------------------------------------------------------

const state = {
  step: 'step-welcome',
  signingIn: false,
  onedriveDriveId: null,
  defaultMountRoot: '~/Cloud',
  libraries: [],
  primarySiteId: null,
  primarySiteName: null,
  authUnlisteners: [],
  finalMounts: [],
};

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

const AUTH_TIMEOUT_SECS = 120;
let countdownTimer = null;

function startCountdown() {
  stopCountdown();
  let remaining = AUTH_TIMEOUT_SECS;
  const el = document.getElementById('auth-countdown');
  el.textContent = 'Temps restant : ' + remaining + 's';
  el.className = 'auth-countdown';

  countdownTimer = setInterval(() => {
    remaining--;
    if (remaining <= 0) {
      stopCountdown();
      el.textContent = '';
      return;
    }
    el.textContent = 'Temps restant : ' + remaining + 's';
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
  'step-welcome': 'Configuration Carmine',
  'step-libraries': 'Sélection des bibliothèques \u2014 Carmine',
  'step-success': 'Terminé \u2014 Carmine',
};

const STEP_MAP = {
  'step-welcome': 1,
  'step-libraries': 2,
  'step-success': 3,
};

function updateStepper(stepId) {
  const currentStep = STEP_MAP[stepId] || 1;
  for (let i = 1; i <= 3; i++) {
    const el = document.getElementById('stepper-' + i);
    if (!el) continue;
    el.classList.remove('active', 'done');
    if (i < currentStep) el.classList.add('done');
    else if (i === currentStep) el.classList.add('active');
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
// Authentification
// ---------------------------------------------------------------------------

function cleanupAuthListeners() {
  state.authUnlisteners.forEach(fn => { try { fn(); } catch (_) {} });
  state.authUnlisteners = [];
}

async function startSignIn() {
  if (state.signingIn) return;

  state.signingIn = true;
  document.getElementById('sign-in-btn').style.display = 'none';
  document.getElementById('auth-waiting').style.display = 'block';
  document.getElementById('auth-error').style.display = 'none';
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
    errEl.textContent = 'Échec de connexion : ' + (event.payload || 'erreur inconnue');
    errEl.style.display = 'block';
  }));

  try {
    const authUrl = await invoke('start_sign_in');
    document.getElementById('auth-url').value = authUrl;
  } catch (e) {
    state.signingIn = false;
    stopCountdown();
    cleanupAuthListeners();
    showStatus('Échec de la connexion', 'error');
    document.getElementById('sign-in-btn').style.display = '';
    document.getElementById('auth-waiting').style.display = 'none';
  }
}

async function cancelSignIn() {
  state.signingIn = false;
  stopCountdown();
  try { await invoke('cancel_sign_in'); } catch (e) { }
  cleanupAuthListeners();
  document.getElementById('sign-in-btn').style.display = '';
  document.getElementById('auth-waiting').style.display = 'none';
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
    btn.textContent = 'Copié !';
    setTimeout(() => { btn.textContent = 'Copier'; }, 2000);
  } catch (e) {
    showStatus('Impossible de copier l\'URL', 'error');
  }
}

async function onSignInComplete() {
  goToStep('step-libraries');
  await loadLibraries();
}

// ---------------------------------------------------------------------------
// Bibliothèques
// ---------------------------------------------------------------------------

async function loadLibraries() {
  document.getElementById('libraries-loading').style.display = 'block';
  document.getElementById('libraries-onedrive-section').style.display = 'none';
  document.getElementById('libraries-sp-section').style.display = 'none';
  document.getElementById('libraries-error').style.display = 'none';

  const [driveResult, librariesResult, siteInfoResult] = await Promise.allSettled([
    invoke('get_drive_info'),
    invoke('list_primary_site_libraries'),
    invoke('get_primary_site_info'),
  ]);

  document.getElementById('libraries-loading').style.display = 'none';

  if (siteInfoResult.status === 'fulfilled') {
    state.primarySiteId = siteInfoResult.value.site_id;
    state.primarySiteName = siteInfoResult.value.site_name;
  }

  if (driveResult.status === 'fulfilled') {
    const drive = driveResult.value;
    state.onedriveDriveId = drive.id;
    document.getElementById('onedrive-drive-name').textContent = drive.name || 'OneDrive';
    document.getElementById('onedrive-mount-path').textContent = state.defaultMountRoot + '/OneDrive';
    document.getElementById('libraries-onedrive-section').style.display = 'block';
  }

  if (librariesResult.status === 'fulfilled') {
    state.libraries = librariesResult.value;
    renderLibraries(librariesResult.value);
    document.getElementById('libraries-sp-section').style.display = 'block';
  }

  if (driveResult.status === 'rejected' && librariesResult.status === 'rejected') {
    const errEl = document.getElementById('libraries-error');
    errEl.textContent = 'Impossible de charger les données du compte. Veuillez vous reconnecter.';
    errEl.style.display = 'block';
  } else if (driveResult.status === 'rejected') {
    showStatus('OneDrive indisponible \u2014 Bibliothèques SharePoint chargées', 'info');
  } else if (librariesResult.status === 'rejected') {
    showStatus('Bibliothèques SharePoint indisponibles \u2014 OneDrive chargé', 'info');
  }

  updateGetStartedBtn();
}

function renderLibraries(libraries) {
  const listEl = document.getElementById('libraries-sp-list');
  listEl.innerHTML = '';

  if (libraries.length === 0) {
    const hint = document.createElement('p');
    hint.className = 'hint';
    hint.textContent = 'Aucune bibliothèque SharePoint trouvée pour votre organisation.';
    listEl.appendChild(hint);
    return;
  }

  libraries.forEach(lib => {
    const row = document.createElement('div');
    row.className = 'lib-row';
    row.dataset.action = 'toggle-lib';
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

    row.appendChild(check);
    row.appendChild(info);
    listEl.appendChild(row);
  });
}

function toggleLibrary(driveId) {
  const row = document.querySelector('.lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
  if (!row) return;
  row.classList.toggle('selected');
  updateGetStartedBtn();
}

// ---------------------------------------------------------------------------
// Finalisation
// ---------------------------------------------------------------------------

function updateGetStartedBtn() {
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    document.getElementById('libraries-onedrive-section').style.display !== 'none';
  const selectedLibs = document.querySelectorAll('#libraries-sp-list .lib-row.selected');
  const hasSelection = onedriveChecked || selectedLibs.length > 0;
  document.getElementById('get-started-btn').disabled = !hasSelection;
}

async function getStarted() {
  const errEl = document.getElementById('libraries-error');
  errEl.style.display = 'none';

  const btn = document.getElementById('get-started-btn');
  const origLabel = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Configuration\u2026';

  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    document.getElementById('libraries-onedrive-section').style.display !== 'none';

  const selectedRows = document.querySelectorAll('#libraries-sp-list .lib-row.selected');
  const selectedLibraryIds = new Set();
  selectedRows.forEach(row => selectedLibraryIds.add(row.dataset.driveId));

  const errors = [];
  let totalMounted = 0;

  if (onedriveChecked && state.onedriveDriveId) {
    try {
      await invoke('add_mount', {
        mountType: 'drive',
        driveId: state.onedriveDriveId,
        mountPoint: state.defaultMountRoot + '/OneDrive',
      });
      totalMounted++;
    } catch (e) {
      errors.push('OneDrive : ' + formatError(e));
    }
  }

  for (const lib of state.libraries) {
    if (!selectedLibraryIds.has(lib.id)) continue;
    const safeName = sanitizePath(lib.name);
    const mountPoint = state.defaultMountRoot + '/' + safeName + '/';
    try {
      await invoke('add_mount', {
        mountType: 'sharepoint',
        mountPoint,
        driveId: lib.id,
        siteId: state.primarySiteId || null,
        siteName: state.primarySiteName || null,
        libraryName: lib.name,
      });
      totalMounted++;
    } catch (e) {
      errors.push(lib.name + ' : ' + formatError(e));
    }
  }

  if (errors.length > 0 && totalMounted === 0) {
    errEl.textContent = 'Échec de l\'ajout des lecteurs : ' + errors.join('; ');
    errEl.style.display = 'block';
    showStatus('Échec de la configuration', 'error');
    btn.disabled = false;
    btn.textContent = origLabel;
    return;
  }

  try {
    await invoke('complete_wizard');
  } catch (e) {
    showStatus('Échec de finalisation', 'error');
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
      li.style.fontSize = '13px';
      li.style.color = 'var(--text-secondary)';
      li.style.marginBottom = '4px';
      li.textContent = m.name + ' \u2192 ' + m.mount_point;
      list.appendChild(li);
    });
  } catch (e) { }
  goToStep('step-success');
}

async function switchAccount() {
  const btn = document.getElementById('switch-account-btn');
  btn.disabled = true;
  btn.textContent = 'Déconnexion\u2026';
  try { await invoke('sign_out'); } catch (e) { }
  btn.disabled = false;
  btn.textContent = 'Changer de compte';
  state.onedriveDriveId = null;
  state.libraries = [];
  document.getElementById('sign-in-btn').style.display = '';
  document.getElementById('auth-waiting').style.display = 'none';
  document.getElementById('auth-url').value = '';
  document.getElementById('auth-error').style.display = 'none';
  goToStep('step-welcome');
}

async function init() {
  try {
    state.defaultMountRoot = await invoke('get_default_mount_root');
    state.defaultMountRoot = state.defaultMountRoot.replace(/[/\\]+$/, '');
  } catch (e) { }

  document.getElementById('sign-in-btn').addEventListener('click', startSignIn);
  document.getElementById('copy-btn').addEventListener('click', copyAuthUrl);
  document.getElementById('cancel-btn').addEventListener('click', cancelSignIn);
  document.getElementById('onedrive-check').addEventListener('change', updateGetStartedBtn);
  document.getElementById('get-started-btn').addEventListener('click', getStarted);
  document.getElementById('wizard-close-btn').addEventListener('click', () => {
    window.__TAURI__.window.getCurrentWindow().close();
  });
  document.getElementById('switch-account-btn').addEventListener('click', switchAccount);

  document.querySelector('.main-content').addEventListener('click', (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    if (target.dataset.action === 'toggle-lib') toggleLibrary(target.dataset.driveId);
  });
}
init();
