// CloudMount shared UI utilities

let _statusTimer = null;

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
