import { Show, type JSX } from 'solid-js';

import { api } from '../ipc';
import { authDegraded } from '../store/auth';
import { WarningIcon } from './Icons';
import { formatError, showStatus } from './StatusBar';

export const AuthBanner = (): JSX.Element => {
  const signIn = async () => {
    try {
      await api.signOut();
      showStatus('Veuillez vous reconnecter', 'info');
    } catch (e) {
      showStatus(formatError(e), 'error');
    }
  };

  return (
    <Show when={authDegraded()}>
      <div class="auth-banner" role="alert">
        <div class="auth-banner-left">
          <WarningIcon class="auth-banner-icon" />
          <span>L'authentification nécessite votre attention.</span>
        </div>
        <button class="btn-ghost btn-sm" type="button" onClick={signIn}>
          Se connecter
        </button>
      </div>
    </Show>
  );
};
