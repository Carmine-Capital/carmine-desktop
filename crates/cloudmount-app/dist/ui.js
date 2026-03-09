// CloudMount shared UI utilities

let _statusTimer = null;

const _errorPatterns = [
  [/GraphApi\s*\{?\s*status:\s*401/i, 'Sign-in expired. Please re-authenticate.'],
  [/GraphApi\s*\{?\s*status:\s*403/i, 'Access denied. Check your permissions.'],
  [/GraphApi\s*\{?\s*status:\s*404/i, 'Resource not found. It may have been deleted.'],
  [/GraphApi\s*\{?\s*status:\s*429/i, 'Too many requests. Please wait a moment.'],
  [/GraphApi\s*\{?\s*status:\s*5\d\d/i, 'Server error. Please try again later.'],
  [/network|fetch|connect|timeout/i, 'Network error. Check your internet connection.'],
  [/token|auth|credential/i, 'Authentication error. Try signing in again.'],
];

function formatError(e) {
  const msg = (e instanceof Error) ? e.message : String(e);
  for (const [re, friendly] of _errorPatterns) {
    if (re.test(msg)) return friendly;
  }
  return msg.replace(/^\w+\s*\{[^}]*message:\s*"([^"]+)".*\}$/, '$1');
}

function showStatus(message, type) {
  const bar = document.getElementById('status-bar');
  if (!bar) return;
  if (_statusTimer) { clearTimeout(_statusTimer); _statusTimer = null; }
  bar.innerHTML = '';

  const text = document.createElement('span');
  text.textContent = typeof message === 'string' ? message : String(message);
  bar.appendChild(text);

  if (type === 'error') {
    const dismiss = document.createElement('button');
    dismiss.className = 'status-dismiss';
    dismiss.textContent = '\u00d7';
    dismiss.setAttribute('aria-label', 'Dismiss');
    dismiss.addEventListener('click', () => { bar.className = ''; });
    bar.appendChild(dismiss);
  }

  bar.className = 'visible ' + type;
  if (type === 'success' || type === 'info') {
    _statusTimer = setTimeout(() => { bar.className = ''; }, 3000);
  }
}
