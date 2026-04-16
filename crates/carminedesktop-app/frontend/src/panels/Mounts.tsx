import { For, Match, Show, Switch, createMemo, onMount, type JSX } from 'solid-js';

import { api } from '../ipc';
import { sanitizePath } from '../utils/format';
import { MountCard } from '../components/MountCard';
import { WarningIcon, DrivesIcon } from '../components/Icons';
import { autoAnimateList } from '../components/autoAnimate';
import { showStatus, formatError } from '../components/StatusBar';
import { confirm } from '../store/confirm';
import {
  addMount,
  bootstrapMounts,
  loadLibraries,
  mountForDrive,
  mounts,
  oneDriveMount,
  removeMount,
} from '../store/mounts';

interface CardItem {
  kind: 'onedrive' | 'sharepoint';
  key: string;
  name: string;
  driveId: string;
  mountId: string | null;
  mountPoint: string | null;
  isMounted: boolean;
}

/** Mounts panel — activates/deactivates libraries.  The bootstrap and library
 *  fetches live in the store; this component just reads from it and wires up
 *  toggle handlers.  Cards use auto-animate for add/remove transitions. */
export const Mounts = (): JSX.Element => {
  onMount(() => {
    // `list_mounts` is cheap and local; running it on every tab visit keeps
    // the card state fresh if a mount was added/removed from the tray menu.
    void bootstrapMounts().catch((e) => showStatus(formatError(e), 'error'));
    void loadLibraries();
  });

  const oneDriveCard = createMemo<CardItem | null>(() => {
    const od = oneDriveMount();
    const info = mounts.driveInfo;
    if (!od && !info) return null;
    return {
      kind: 'onedrive',
      key: 'onedrive',
      name: 'OneDrive',
      driveId: info?.id ?? od?.drive_id ?? '',
      mountId: od?.id ?? null,
      mountPoint: od?.mount_point ?? null,
      isMounted: !!od,
    };
  });

  const sharepointCards = createMemo<CardItem[]>(() =>
    mounts.libraries.map((lib) => {
      const existing = mountForDrive(lib.id);
      return {
        kind: 'sharepoint',
        key: `sp:${lib.id}`,
        name: lib.name,
        driveId: lib.id,
        mountId: existing?.id ?? null,
        mountPoint: existing?.mount_point ?? null,
        isMounted: !!existing,
      };
    }),
  );

  const onToggle = async (card: CardItem, next: boolean) => {
    if (next) {
      const mountRoot = await api.getDefaultMountRoot();
      if (card.kind === 'sharepoint') {
        const site = mounts.primarySite;
        await addMount({
          mountType: 'sharepoint',
          mountPoint: `${mountRoot}/${sanitizePath(card.name)}`,
          driveId: card.driveId,
          siteId: site?.site_id ?? null,
          siteName: site?.site_name ?? null,
          libraryName: card.name,
        });
      } else {
        await addMount({
          mountType: 'drive',
          mountPoint: `${mountRoot}/OneDrive`,
          driveId: card.driveId,
        });
      }
      showStatus(`${card.name} activé`, 'success');
    } else if (card.mountId) {
      const ok = await confirm({
        title: 'Retirer ce lecteur ?',
        message:
          'Les fichiers locaux de ce lecteur seront supprimés. Les fichiers distants restent intacts.',
        confirmLabel: 'Retirer',
        danger: true,
      });
      if (!ok) {
        // Re-throw a sentinel so MountCard's try/catch flips the toggle back.
        throw new Error('__cancelled__');
      }
      await removeMount(card.mountId);
      showStatus(`${card.name} désactivé`, 'success');
    }
  };

  const hasAny = createMemo(
    () => oneDriveCard() !== null || sharepointCards().length > 0,
  );

  return (
    <>
      <p class="section-heading">Bibliothèques actives</p>
      <ul class="mount-list" ref={autoAnimateList}>
        <Show when={oneDriveCard()}>
          {(card) => (
            <MountCard
              kind="onedrive"
              name={card().name}
              mountPoint={card().mountPoint}
              isMounted={card().isMounted}
              onToggle={(next) => onToggle(card(), next)}
            />
          )}
        </Show>
        <Switch>
          <Match when={mounts.librariesStatus === 'loading'}>
            <LibraryEmpty spinner title="Chargement des bibliothèques…" />
          </Match>
          <Match when={mounts.librariesStatus === 'error'}>
            <LibraryEmpty
              icon={<WarningIcon size={32} />}
              title="Impossible de charger les bibliothèques"
              hint="Vérifiez votre connexion et réessayez."
            />
          </Match>
          <Match when={mounts.librariesStatus === 'ready'}>
            <For each={sharepointCards()}>
              {(card) => (
                <MountCard
                  kind="sharepoint"
                  name={card.name}
                  mountPoint={card.mountPoint}
                  isMounted={card.isMounted}
                  onToggle={(next) => onToggle(card, next)}
                />
              )}
            </For>
            <Show when={!hasAny()}>
              <LibraryEmpty
                icon={<DrivesIcon size={32} />}
                title="Aucune bibliothèque disponible"
                hint="Connectez-vous ou sélectionnez des bibliothèques depuis l’assistant d’installation."
              />
            </Show>
          </Match>
        </Switch>
      </ul>
    </>
  );
};

interface LibraryEmptyProps {
  spinner?: boolean;
  icon?: JSX.Element;
  title: string;
  hint?: string;
}

const LibraryEmpty = (props: LibraryEmptyProps): JSX.Element => (
  <li class="library-empty">
    <Show when={props.spinner}>
      <span class="spinner" />
    </Show>
    <Show when={!props.spinner && props.icon}>
      <div class="library-empty-icon" aria-hidden="true">
        {props.icon}
      </div>
    </Show>
    <div class="library-empty-title">{props.title}</div>
    <Show when={props.hint}>
      <div class="library-empty-hint">{props.hint}</div>
    </Show>
  </li>
);
