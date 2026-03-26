const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const state = {
  step: 'step-welcome',
  signingIn: false,
  onedriveDriveId: null,
  defaultMountRoot: '~/Cloud',
  libraries: [],
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
  'step-libraries': 'Select Libraries \u2014 Carmine Desktop Setup',
  'step-success': 'All Set \u2014 Carmine Desktop Setup',
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
  const footer = document.getElementById('wizard-footer');
  if (footer) {
    footer.textContent = '';
    footer.style.display = 'none';
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

  const fuseWarning = document.getElementById('fuse-warning');
  fuseWarning.style.display = 'none';

  try {
    const fuseOk = await invoke('check_fuse_available');
    if (!fuseOk) {
      fuseWarning.textContent = 'FUSE is not installed. Install libfuse3 (Linux) or macFUSE (macOS) to use Carmine Desktop.';
      fuseWarning.style.display = 'block';
      showStatus('FUSE is not installed. Install libfuse3 (Linux) or macFUSE (macOS) to use Carmine Desktop.', 'error');
      return;
    }
  } catch (e) {
    console.warn('FUSE check failed, proceeding:', e);
  }

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
    document.getElementById('sign-in-btn').style.display = '';
    document.getElementById('auth-waiting').style.display = 'none';
  }
}

async function cancelSignIn() {
  state.signingIn = false;
  stopCountdown();
  try { await invoke('cancel_sign_in'); } catch (e) { console.warn('cancel failed:', e); }
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
    btn.textContent = 'Copied!';
    setTimeout(() => { btn.textContent = 'Copy URL'; }, 2000);
  } catch (e) {
    console.error('clipboard write failed:', e);
    showStatus('Could not copy URL', 'error');
  }
}

async function onSignInComplete() {
  goToStep('step-libraries');
  await loadLibraries();
}

// ---------------------------------------------------------------------------
// Libraries — loading
// ---------------------------------------------------------------------------

async function loadLibraries() {
  document.getElementById('libraries-loading').style.display = 'block';
  document.getElementById('libraries-onedrive-section').style.display = 'none';
  document.getElementById('libraries-sp-section').style.display = 'none';
  document.getElementById('libraries-error').style.display = 'none';

  const [driveResult, librariesResult] = await Promise.allSettled([
    invoke('get_drive_info'),
    invoke('list_primary_site_libraries'),
  ]);

  document.getElementById('libraries-loading').style.display = 'none';

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
    errEl.textContent = 'Could not load account data \u2014 please try signing in again.';
    errEl.style.display = 'block';
  } else if (driveResult.status === 'rejected') {
    showStatus('OneDrive info unavailable \u2014 SharePoint libraries loaded', 'info');
  } else if (librariesResult.status === 'rejected') {
    showStatus('SharePoint libraries unavailable \u2014 OneDrive loaded', 'info');
  }

  updateGetStartedBtn();
}

// ---------------------------------------------------------------------------
// Libraries — rendering
// ---------------------------------------------------------------------------

function renderLibraries(libraries) {
  const listEl = document.getElementById('libraries-sp-list');
  listEl.innerHTML = '';

  if (libraries.length === 0) {
    const hint = document.createElement('p');
    hint.className = 'hint';
    hint.textContent = 'No SharePoint libraries found for your organization.';
    listEl.appendChild(hint);
    return;
  }

  libraries.forEach(lib => {
    const row = document.createElement('div');
    row.className = 'lib-row';
    row.dataset.action = 'toggle-lib';
    row.dataset.driveId = lib.id;
    row.setAttribute('role', 'checkbox');
    row.setAttribute('aria-checked', 'false');
    row.setAttribute('tabindex', '0');

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

    row.appendChild(check);
    row.appendChild(info);
    listEl.appendChild(row);
  });
}

function toggleLibrary(driveId) {
  const row = document.querySelector('.lib-row[data-drive-id="' + CSS.escape(driveId) + '"]');
  if (!row) return;
  const isSelected = row.classList.toggle('selected');
  row.setAttribute('aria-checked', isSelected ? 'true' : 'false');
  updateGetStartedBtn();
}

// ---------------------------------------------------------------------------
// Get Started / Complete
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
  btn.textContent = 'Setting up\u2026';

  const onedriveSection = document.getElementById('libraries-onedrive-section');
  const onedriveChecked = document.getElementById('onedrive-check') &&
    document.getElementById('onedrive-check').checked &&
    onedriveSection.style.display !== 'none';

  // Collect selected SharePoint libraries
  const selectedRows = document.querySelectorAll('#libraries-sp-list .lib-row.selected');
  const selectedLibraryIds = new Set();
  selectedRows.forEach(row => selectedLibraryIds.add(row.dataset.driveId));

  const errors = [];
  let totalMounted = 0;

  // Mount OneDrive if checked
  if (onedriveChecked && state.onedriveDriveId) {
    try {
      await invoke('add_mount', {
        mountType: 'drive',
        driveId: state.onedriveDriveId,
        mountPoint: state.defaultMountRoot + '/OneDrive',
      });
      totalMounted++;
    } catch (e) {
      errors.push('OneDrive: ' + formatError(e));
    }
  }

  // Mount selected SharePoint libraries
  for (const lib of state.libraries) {
    if (!selectedLibraryIds.has(lib.id)) continue;
    const safeName = sanitizePath(lib.name);
    const mountPoint = state.defaultMountRoot + '/' + safeName + '/';
    try {
      await invoke('add_mount', {
        mountType: 'sharepoint',
        mountPoint,
        driveId: lib.id,
        libraryName: lib.name,
      });
      totalMounted++;
    } catch (e) {
      errors.push(lib.name + ': ' + formatError(e));
    }
  }

  if (errors.length > 0 && totalMounted === 0) {
    errEl.textContent = 'Failed to add mounts: ' + errors.join('; ');
    errEl.style.display = 'block';
    showStatus('Failed to set up mounts', 'error');
    btn.disabled = false;
    btn.textContent = origLabel;
    return;
  }

  if (errors.length > 0) {
    showStatus(totalMounted + ' mounted, ' + errors.length + ' failed', 'info');
  }

  try {
    await invoke('complete_wizard');
  } catch (e) {
    showStatus('Failed to complete setup', 'error');
    btn.disabled = false;
    btn.textContent = origLabel;
    return;
  }

  // Show done step with mount summary
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
  state.onedriveDriveId = null;
  state.libraries = [];
  // Reset auth UI on welcome step
  document.getElementById('sign-in-btn').style.display = '';
  document.getElementById('auth-waiting').style.display = 'none';
  document.getElementById('auth-url').value = '';
  document.getElementById('auth-error').style.display = 'none';
  document.getElementById('fuse-warning').style.display = 'none';
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
  document.getElementById('onedrive-check').addEventListener('change', updateGetStartedBtn);
  document.getElementById('get-started-btn').addEventListener('click', getStarted);
  document.getElementById('wizard-close-btn').addEventListener('click', () => {
    window.__TAURI__.window.getCurrentWindow().close();
  });
  document.getElementById('switch-account-btn').addEventListener('click', switchAccount);

  // Delegation for dynamic library rows
  document.querySelector('.main-content').addEventListener('click', (e) => {
    const target = e.target.closest('[data-action]');
    if (!target) return;
    if (target.dataset.action === 'toggle-lib') toggleLibrary(target.dataset.driveId);
  });
  // Keyboard support for delegated rows
  document.querySelector('.main-content').addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    const target = e.target.closest('[data-action]');
    if (target) { e.preventDefault(); target.click(); }
  });
}
init();
