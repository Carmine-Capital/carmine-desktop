// Utilitaires UI partagés pour Carmine Desktop

let _statusTimer = null;

const _errorPatterns = [
  [/GraphApi\s*\{?\s*status:\s*401/i, 'Session expirée. Veuillez vous reconnecter.'],
  [/GraphApi\s*\{?\s*status:\s*403/i, 'Accès refusé. Vérifiez vos permissions.'],
  [/GraphApi\s*\{?\s*status:\s*404/i, 'Ressource non trouvée. Elle a peut-être été supprimée.'],
  [/GraphApi\s*\{?\s*status:\s*429/i, 'Trop de requêtes. Veuillez patienter un instant.'],
  [/GraphApi\s*\{?\s*status:\s*5\d\d/i, 'Erreur serveur. Veuillez réessayer plus tard.'],
  [/network|fetch|connect|timeout/i, 'Erreur réseau. Vérifiez votre connexion internet.'],
  [/token|auth|credential/i, 'Erreur d\'authentification. Essayez de vous reconnecter.'],
];

function sanitizePath(name) {
  return name.replace(/[/\\:*?"<>|]/g, '_').trim() || '_';
}

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
    dismiss.setAttribute('aria-label', 'Fermer');
    dismiss.addEventListener('click', () => { bar.className = ''; });
    bar.appendChild(dismiss);
  }

  bar.className = 'visible ' + type;
  if (type === 'success' || type === 'info') {
    _statusTimer = setTimeout(() => { bar.className = ''; }, 4000);
  }
}
