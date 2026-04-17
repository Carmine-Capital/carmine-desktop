import { For, Show, type JSX } from 'solid-js';

import { pins } from '../store/pins';
import { PinCard } from '../components/PinCard';
import { Skeleton } from '../components/Skeleton';
import { EmptyOfflineIcon } from '../components/Icons';
import { autoAnimateList } from '../components/autoAnimate';

const OfflineEmpty = (): JSX.Element => (
  <li class="pin-empty">
    <EmptyOfflineIcon />
    <div class="pin-empty-title">Aucun dossier hors-ligne</div>
    <div class="pin-empty-hint">
      Épinglez un dossier depuis l’Explorateur pour le rendre disponible sans connexion.
    </div>
  </li>
);

/** Offline panel — pinned-folders list.  Pin-level settings (default TTL,
 *  storage limits, cache directory) live in Paramètres > Hors-ligne so the
 *  main view stays focused on what the user can act on day-to-day. */
export const Offline = (): JSX.Element => (
  <>
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
