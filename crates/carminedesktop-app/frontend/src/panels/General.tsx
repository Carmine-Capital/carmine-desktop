import { Show, createMemo, createSignal, onCleanup, type JSX } from 'solid-js';

import { api } from '../ipc';
import { formatError, showStatus } from '../components/StatusBar';
import { Skeleton } from '../components/Skeleton';
import { Collapsible } from '../components/Collapsible';
import { Select, type SelectOption } from '../components/Select';
import { confirm } from '../store/confirm';
import { pushToast } from '../store/toasts';
import { parseSizeToBytes } from '../utils/format';
import {
  saveSettings,
  setAutoStart,
  setCacheDir,
  setCacheMaxSize,
  setExplorerNavPane,
  setLogLevel,
  setNotifications,
  setOfflineMaxFolderSize,
  setSyncIntervalSecs,
  settings,
} from '../store/settings';

const SYNC_INTERVALS: SelectOption<number>[] = [
  { value: 30, label: '30 secondes' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
];

const LOG_LEVELS: SelectOption<string>[] = [
  { value: 'trace', label: 'Trace' },
  { value: 'debug', label: 'Debug' },
  { value: 'info', label: 'Info' },
  { value: 'warn', label: 'Avertissement' },
  { value: 'error', label: 'Erreur' },
];

const CACHE_SIZE_PRESETS: SelectOption<string>[] = [
  { value: '5Go', label: '5 Go' },
  { value: '10Go', label: '10 Go' },
  { value: '15Go', label: '15 Go' },
  { value: '20Go', label: '20 Go' },
];

const OFFLINE_FOLDER_PRESETS: SelectOption<string>[] = [
  { value: '1Go', label: '1 Go' },
  { value: '5Go', label: '5 Go' },
  { value: '10Go', label: '10 Go' },
  { value: '25Go', label: '25 Go' },
  { value: '50Go', label: '50 Go' },
];

/** Map a legacy/custom size string (e.g. "5GB", "7Go") onto the closest preset
 *  value so the dropdown always displays something coherent.  Fallback to the
 *  first option when input is unparseable. */
function matchSizePreset(input: string, options: SelectOption<string>[]): string {
  const target = parseSizeToBytes(input);
  if (target <= 0) return options[0]!.value;
  let best = options[0]!.value;
  let bestDiff = Infinity;
  for (const opt of options) {
    const diff = Math.abs(parseSizeToBytes(opt.value) - target);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = opt.value;
    }
  }
  return best;
}

/** General panel — account-independent app preferences.  Toggles and the sync
 *  interval flush on change; free-text inputs debounce so typing stays smooth.
 *  Persistence is best-effort: a failure surfaces in the status bar but the
 *  local value stays where the user put it so they can retry. */
export const General = (): JSX.Element => {
  let debounce: number | null = null;
  const flushDebounced = () => {
    if (debounce !== null) {
      clearTimeout(debounce);
      debounce = null;
    }
  };

  const scheduleSave = () => {
    flushDebounced();
    debounce = window.setTimeout(() => {
      debounce = null;
      void persist();
    }, 500);
  };

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
  const onSyncInterval = (value: number) => {
    setSyncIntervalSecs(value);
    void persist();
  };
  const onCacheDir = (value: string) => {
    setCacheDir(value);
    scheduleSave();
  };
  const onCacheMax = (value: string) => {
    setCacheMaxSize(value);
    void persist();
  };
  const onOfflineMaxFolder = (value: string) => {
    setOfflineMaxFolderSize(value);
    void persist();
  };
  const onLogLevel = (value: string) => {
    setLogLevel(value);
    void persist();
  };
  const onExplorerNavPane = (value: boolean) => {
    setExplorerNavPane(value);
    void persist();
  };

  const cacheMaxValue = createMemo(() =>
    matchSizePreset(settings.cacheMaxSize, CACHE_SIZE_PRESETS),
  );
  const offlineMaxValue = createMemo(() =>
    matchSizePreset(settings.offlineMaxFolderSize, OFFLINE_FOLDER_PRESETS),
  );

  onCleanup(() => {
    // A pending edit should still be persisted if the user navigates away.
    if (debounce !== null) {
      clearTimeout(debounce);
      debounce = null;
      void persist();
    }
  });

  const [clearing, setClearing] = createSignal(false);
  const clearCache = async () => {
    const ok = await confirm({
      title: 'Vider le cache ?',
      message:
        'Les fichiers temporaires seront supprimés. Vos dossiers hors-ligne sont préservés et retéléchargés automatiquement.',
      confirmLabel: 'Vider',
      danger: true,
    });
    if (!ok) return;
    if (clearing()) return;
    setClearing(true);
    try {
      await api.clearCache();
      pushToast({ kind: 'success', title: 'Cache vidé' });
    } catch (e) {
      pushToast({
        kind: 'error',
        title: 'Échec du vidage du cache',
        message: formatError(e),
      });
    } finally {
      setClearing(false);
    }
  };

  return (
    <>
      <Show
        when={settings.loaded}
        fallback={<Skeleton label="Chargement des préférences…" />}
      >
        <p class="section-heading">Général</p>
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

        <p class="section-heading">Stockage local</p>
        <div class="setting-row">
          <div class="setting-label">
            <div class="label-text">Limite du cache</div>
            <div class="label-sub">
              Espace disque maximal pour les fichiers temporaires (les dossiers hors-ligne ne comptent pas)
            </div>
          </div>
          <div class="setting-control">
            <Select
              value={cacheMaxValue()}
              options={CACHE_SIZE_PRESETS}
              onChange={onCacheMax}
              ariaLabel="Limite du cache"
            />
            <button
              type="button"
              class="btn-danger"
              disabled={clearing()}
              aria-busy={clearing()}
              onClick={clearCache}
            >
              {clearing() ? 'Vidage…' : 'Vider'}
            </button>
          </div>
        </div>

        <Collapsible title="Avancé" defaultOpen={false}>
          <div class="setting-row">
            <div class="setting-label">
              <div class="label-text">Intervalle de synchro</div>
              <div class="label-sub">Fréquence de vérification des changements</div>
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

          <div class="setting-row">
            <div class="setting-label">
              <div class="label-text">Dossier de cache</div>
              <div class="label-sub">Emplacement des fichiers temporaires</div>
            </div>
            <div class="setting-control">
              <input
                type="text"
                placeholder="Par défaut"
                value={settings.cacheDir}
                onInput={(e) => onCacheDir(e.currentTarget.value)}
                aria-label="Dossier de cache"
              />
            </div>
          </div>

          <div class="setting-row">
            <div class="setting-label">
              <div class="label-text">Taille max d’un dossier hors-ligne</div>
              <div class="label-sub">
                Taille maximale autorisée par dossier épinglé (hors limite du cache)
              </div>
            </div>
            <div class="setting-control">
              <Select
                value={offlineMaxValue()}
                options={OFFLINE_FOLDER_PRESETS}
                onChange={onOfflineMaxFolder}
                ariaLabel="Taille max d’un dossier hors-ligne"
              />
            </div>
          </div>

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
    </>
  );
};
