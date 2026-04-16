// Legacy `showStatus` helper retained as a thin shim over the toast store so
// existing call sites (WizardApp, App, panels, other components) keep working
// without touching them.  All user feedback now flows through `pushToast`
// rendered by `ToastHost`; this module no longer renders any UI of its own.
//
// `formatError` still lives here because half the callers import it alongside
// `showStatus` and move-for-move relocation would ripple across many files
// for no behavioural gain.

import { type JSX } from 'solid-js';

import { pushToast, type ToastKind } from '../store/toasts';

export type StatusKind = 'success' | 'error' | 'info';

/** Push a transient toast.  `kind` is widened to `ToastKind` internally so the
 *  shim forwards cleanly even though the legacy type only covered three kinds.
 */
export function showStatus(message: string, kind: StatusKind): void {
  pushToast({ kind: kind as ToastKind, title: message });
}

/** Kept as an exported no-op render for backwards compatibility with `App.tsx`
 *  and `WizardApp.tsx` which still mount `<StatusBar />`.  The real UI is
 *  `<ToastHost />`; removing the mount points is left to a follow-up so this
 *  change stays surgical. */
export const StatusBar = (): JSX.Element => null;

const errorPatterns: [RegExp, string][] = [
  [/GraphApi\s*\{?\s*status:\s*401/i, 'Session expirée. Veuillez vous reconnecter.'],
  [/GraphApi\s*\{?\s*status:\s*403/i, 'Accès refusé. Vérifiez vos permissions.'],
  [/GraphApi\s*\{?\s*status:\s*404/i, 'Ressource non trouvée. Elle a peut-être été supprimée.'],
  [/GraphApi\s*\{?\s*status:\s*429/i, 'Trop de requêtes. Veuillez patienter un instant.'],
  [/GraphApi\s*\{?\s*status:\s*5\d\d/i, 'Erreur serveur. Veuillez réessayer plus tard.'],
  [/network|fetch|connect|timeout/i, 'Erreur réseau. Vérifiez votre connexion internet.'],
  [/token|auth|credential/i, "Erreur d'authentification. Essayez de vous reconnecter."],
];

export function formatError(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  for (const [re, friendly] of errorPatterns) {
    if (re.test(msg)) return friendly;
  }
  return msg.replace(/^\w+\s*\{[^}]*message:\s*"([^"]+)".*\}$/, '$1');
}
