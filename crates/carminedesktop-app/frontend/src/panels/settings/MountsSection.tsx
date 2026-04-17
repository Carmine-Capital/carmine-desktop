import { Show, type JSX } from 'solid-js';

import { Collapsible } from '../../components/Collapsible';
import { Select, type SelectOption } from '../../components/Select';
import { formatError, showStatus } from '../../components/StatusBar';
import {
  saveSettings,
  setExplorerNavPane,
  setSyncIntervalSecs,
  settings,
} from '../../store/settings';

const SYNC_INTERVALS: SelectOption<number>[] = [
  { value: 30, label: '30 secondes' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
];

export const MountsSection = (): JSX.Element => {
  const persist = async () => {
    try {
      await saveSettings();
    } catch (e) {
      showStatus(formatError(e), 'error');
    }
  };

  const onSyncInterval = (value: number) => {
    setSyncIntervalSecs(value);
    void persist();
  };
  const onExplorerNavPane = (value: boolean) => {
    setExplorerNavPane(value);
    void persist();
  };

  return (
    <Show when={settings.loaded}>
      <div class="setting-row">
        <div class="setting-label">
          <div class="label-text">Intervalle de synchro</div>
          <div class="label-sub">Fréquence de vérification des changements distants</div>
        </div>
        <div class="setting-control">
          <Select
            value={settings.syncIntervalSecs}
            options={SYNC_INTERVALS}
            onChange={onSyncInterval}
            ariaLabel="Intervalle de synchronisation"
          />
        </div>
      </div>

      <Collapsible title="Avancé" defaultOpen={false}>
        <div class="setting-row">
          <div class="setting-label">
            <div class="label-text">Afficher dans l’Explorateur</div>
            <div class="label-sub">
              Épingler Carmine dans le volet de navigation Windows
            </div>
          </div>
          <div class="setting-control">
            <label class="toggle-switch">
              <input
                type="checkbox"
                checked={settings.explorerNavPane}
                onChange={(e) => onExplorerNavPane(e.currentTarget.checked)}
                aria-label="Afficher dans l’Explorateur"
              />
              <span class="toggle-track" aria-hidden="true" />
            </label>
          </div>
        </div>
      </Collapsible>
    </Show>
  );
};
