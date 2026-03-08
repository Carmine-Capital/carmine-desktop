// CloudMount shared UI utilities

let _statusTimer = null;

function showStatus(message, type) {
  const bar = document.getElementById('status-bar');
  if (!bar) return;
  if (_statusTimer) { clearTimeout(_statusTimer); _statusTimer = null; }
  bar.textContent = typeof message === 'string' ? message : String(message);
  bar.className = 'visible ' + type;
  if (type === 'success' || type === 'info') {
    _statusTimer = setTimeout(() => { bar.className = ''; }, 3000);
  }
}
