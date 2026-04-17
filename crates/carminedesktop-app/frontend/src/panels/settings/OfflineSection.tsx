import { Show, createMemo, createSignal, onCleanup, type JSX } from 'solid-js';

import { api } from '../../ipc';
import { PinDurationPicker } from '../../components/PinDurationPicker';
import { DEFAULT_PIN_DURATION_OPTIONS } from '../../components/pinDurationOptions';
import { Collapsible } from '../../components/Collapsible';
import { Select, type SelectOption } from '../../components/Select';
import { formatError, showStatus } from '../../components/StatusBar';
import { confirm } from '../../store/confirm';
import { pushToast } from '../../store/toasts';
import { parseSizeToBytes } from '../../utils/format';
import {
  saveSettings,
  setCacheDir,
  setCacheMaxSize,
  setOfflineMaxFolderSize,
  setOfflineTtlSecs,
  settings,
} from '../../store/settings';

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

export const OfflineSection = (): JSX.Element => {
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

  const onDefaultTtl = (secs: number) => {
    setOfflineTtlSecs(secs);
    void persist();
  };
  const onOfflineMaxFolder = (value: string) => {
    setOfflineMaxFolderSize(value);
    void persist();
  };
  const onCacheMax = (value: string) => {
    setCacheMaxSize(value);
    void persist();
  };
  const onCacheDir = (value: string) => {
    setCacheDir(value);
    scheduleSave();
  };

  const cacheMaxValue = createMemo(() =>
    matchSizePreset(settings.cacheMaxSize, CACHE_SIZE_PRESETS),
  );
  const offlineMaxValue = createMemo(() =>
    matchSizePreset(settings.offlineMaxFolderSize, OFFLINE_FOLDER_PRESETS),
  );

  onCleanup(() => {
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
    <Show when={settings.loaded}>
      <div class="setting-row">
        <div class="setting-label">
          <div class="label-text">Durée par défaut</div>
          <div class="label-sub">Appliquée aux nouveaux dossiers épinglés</div>
        </div>
        <div class="setting-control">
          <PinDurationPicker
            value={settings.offlineTtlSecs}
            onChange={onDefaultTtl}
            options={DEFAULT_PIN_DURATION_OPTIONS}
            ariaLabel="Durée par défaut des épinglages"
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
      </Collapsible>
    </Show>
  );
};
