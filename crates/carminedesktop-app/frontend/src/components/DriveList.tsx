import { For, Show, type JSX } from 'solid-js';

import { drives } from '../store/drives';
import { autoAnimateList } from './autoAnimate';
import { DriveCard } from './DriveCard';

export const DriveList = (): JSX.Element => (
  <Show
    when={drives.list.length > 0}
    fallback={
      <Show when={drives.loaded}>
        <div class="mount-empty">Aucun lecteur monté</div>
      </Show>
    }
  >
    <div class="drive-cards" ref={autoAnimateList}>
      <For each={drives.list}>{(drive) => <DriveCard drive={drive} />}</For>
    </div>
  </Show>
);
