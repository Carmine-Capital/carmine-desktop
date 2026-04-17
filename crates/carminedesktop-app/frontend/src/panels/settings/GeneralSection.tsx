import { Show, type JSX } from 'solid-js';

import { formatError, showStatus } from '../../components/StatusBar';
import { Skeleton } from '../../components/Skeleton';
import { Collapsible } from '../../components/Collapsible';
import { Select, type SelectOption } from '../../components/Select';
import {
  saveSettings,
  setAutoStart,
  setLogLevel,
  setNotifications,
  settings,
} from '../../store/settings';

const LOG_LEVELS: SelectOption<string>[] = [
  { value: 'trace', label: 'Trace' },
  { value: 'debug', label: 'Debug' },
  { value: 'info', label: 'Info' },
  { value: 'warn', label: 'Avertissement' },
  { value: 'error', label: 'Erreur' },
];

export const GeneralSection = (): JSX.Element => {
  const persist = async () => {
    try {
      await saveSettings();
    } catch (e) {
      showStatus(formatError(e), 'error');
    }
  };

  const onAutoStart = (value: boolean) => {
    setAutoStart(value);
    void persist();
  };
  const onNotifications = (value: boolean) => {
    setNotifications(value);
    void persist();
  };
  const onLogLevel = (value: string) => {
    setLogLevel(value);
    void persist();
  };

  return (
    <Show
      when={settings.loaded}
      fallback={<Skeleton label="Chargement des préférences…" />}
    >
      <div class="setting-row">
        <div class="setting-label">
          <div class="label-text">Lancer au démarrage</div>
          <div class="label-sub">Ouvrir Carmine lors de l’ouverture de session</div>
        </div>
        <div class="setting-control">
          <label class="toggle-switch">
            <input
              type="checkbox"
              checked={settings.autoStart}
              onChange={(e) => onAutoStart(e.currentTarget.checked)}
              aria-label="Lancer au démarrage"
            />
            <span class="toggle-track" aria-hidden="true" />
          </label>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-label">
          <div class="label-text">Notifications</div>
          <div class="label-sub">Afficher les alertes de synchronisation</div>
        </div>
        <div class="setting-control">
          <label class="toggle-switch">
            <input
              type="checkbox"
              checked={settings.notifications}
              onChange={(e) => onNotifications(e.currentTarget.checked)}
              aria-label="Notifications"
            />
            <span class="toggle-track" aria-hidden="true" />
          </label>
        </div>
      </div>

      <Collapsible title="Avancé" defaultOpen={false}>
        <div class="setting-row">
          <div class="setting-label">
            <div class="label-text">Niveau de log</div>
            <div class="label-sub">Verbosité des journaux de diagnostic</div>
          </div>
          <div class="setting-control">
            <Select
              value={settings.logLevel}
              options={LOG_LEVELS}
              onChange={onLogLevel}
              ariaLabel="Niveau de log"
            />
          </div>
        </div>
      </Collapsible>
    </Show>
  );
};
