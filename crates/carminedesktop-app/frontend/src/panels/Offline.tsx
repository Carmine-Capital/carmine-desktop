import { For, Show, type JSX } from 'solid-js';

import { pins } from '../store/pins';
import { PinCard } from '../components/PinCard';
import { PinDurationPicker } from '../components/PinDurationPicker';
import { DEFAULT_PIN_DURATION_OPTIONS } from '../components/pinDurationOptions';
import { Skeleton } from '../components/Skeleton';
import { EmptyOfflineIcon } from '../components/Icons';
import { autoAnimateList } from '../components/autoAnimate';
import { formatError, showStatus } from '../components/StatusBar';
import { saveSettings, setOfflineTtlSecs, settings } from '../store/settings';

const OfflineEmpty = (): JSX.Element => (
  <li class="pin-empty">
    <EmptyOfflineIcon />
    <div class="pin-empty-title">Aucun dossier hors-ligne</div>
    <div class="pin-empty-hint">
      Épinglez un dossier depuis l’Explorateur pour le rendre disponible sans connexion.
    </div>
  </li>
);

/** Offline panel — pure view.  `attachPinEvents` + `bootstrapPins` are wired
 *  once at App mount so pins stay live regardless of which tab is active. */
export const Offline = (): JSX.Element => {
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

  return (
    <>
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
      </Show>

      <p class="section-heading">Dossiers épinglés</p>
      <Show when={!pins.loaded}>
        <ul class="pin-list">
          <Skeleton variant="pin" tag="li" label="Chargement…" />
        </ul>
      </Show>
      <Show when={pins.loaded}>
        <ul class="pin-list" ref={autoAnimateList}>
          <For each={pins.list} fallback={<OfflineEmpty />}>
            {(pin) => <PinCard pin={pin} />}
          </For>
        </ul>
      </Show>
    </>
  );
};
