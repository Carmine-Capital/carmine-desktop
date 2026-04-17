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
import type { DriveInfo } from '../bindings';

/** Mounts panel — activates/deactivates libraries. Renders a stable `<For>` over
 *  the store's library list; each row resolves its own mount state reactively
 *  so toggling a library updates only that card's pill/path instead of
 *  remounting every sibling (which used to replay the `.mount-card` fade-in and
 *  felt like a hard refresh). */
export const Mounts = (): JSX.Element => {
  onMount(() => {
    // `list_mounts` is cheap and local; running it on every tab visit keeps
    // the card state fresh if a mount was added/removed from the tray menu.
    void bootstrapMounts().catch((e) => showStatus(formatError(e), 'error'));
    void loadLibraries();
  });

  const addSharepoint = async (lib: DriveInfo) => {
    const mountRoot = await api.getDefaultMountRoot();
    const site = mounts.primarySite;
    await addMount({
      mountType: 'sharepoint',
      mountPoint: `${mountRoot}/${sanitizePath(lib.name)}`,
      driveId: lib.id,
      siteId: site?.site_id ?? null,
      siteName: site?.site_name ?? null,
      libraryName: lib.name,
    });
  };

  const addOneDrive = async (driveId: string) => {
    const mountRoot = await api.getDefaultMountRoot();
    await addMount({
      mountType: 'drive',
      mountPoint: `${mountRoot}/OneDrive`,
      driveId,
    });
  };

  const removeWithConfirm = async (mountId: string) => {
    const ok = await confirm({
      title: 'Retirer ce lecteur ?',
      message:
        'Les fichiers locaux de ce lecteur seront supprimés. Les fichiers distants restent intacts.',
      confirmLabel: 'Retirer',
      danger: true,
    });
    if (!ok) {
      // Sentinel so MountCard's try/catch flips the toggle back.
      throw new Error('__cancelled__');
    }
    await removeMount(mountId);
  };

  const hasAny = createMemo(() => {
    const hasOneDrive = !!oneDriveMount() || !!mounts.driveInfo;
    return hasOneDrive || mounts.libraries.length > 0;
  });

  return (
    <>
      <p class="section-heading">Bibliothèques actives</p>
      <ul class="mount-list" ref={autoAnimateList}>
        <Show when={oneDriveMount() || mounts.driveInfo}>
          <OneDriveRow addOneDrive={addOneDrive} removeWithConfirm={removeWithConfirm} />
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
            <For each={mounts.libraries}>
              {(lib) => (
                <SharePointLibraryRow
                  lib={lib}
                  addSharepoint={addSharepoint}
                  removeWithConfirm={removeWithConfirm}
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

interface SharePointLibraryRowProps {
  lib: DriveInfo;
  addSharepoint: (lib: DriveInfo) => Promise<void>;
  removeWithConfirm: (mountId: string) => Promise<void>;
}

const SharePointLibraryRow = (props: SharePointLibraryRowProps): JSX.Element => {
  const mount = createMemo(() => mountForDrive(props.lib.id));

  const onToggle = async (next: boolean) => {
    if (next) {
      await props.addSharepoint(props.lib);
      showStatus(`${props.lib.name} activé`, 'success');
    } else {
      const current = mount();
      if (!current) return;
      await props.removeWithConfirm(current.id);
      showStatus(`${props.lib.name} désactivé`, 'success');
    }
  };

  return (
    <MountCard
      kind="sharepoint"
      name={props.lib.name}
      mountPoint={mount()?.mount_point ?? null}
      isMounted={!!mount()}
      onToggle={onToggle}
    />
  );
};

interface OneDriveRowProps {
  addOneDrive: (driveId: string) => Promise<void>;
  removeWithConfirm: (mountId: string) => Promise<void>;
}

const OneDriveRow = (props: OneDriveRowProps): JSX.Element => {
  const mount = createMemo(() => oneDriveMount());
  const driveId = createMemo(() => mounts.driveInfo?.id ?? mount()?.drive_id ?? '');

  const onToggle = async (next: boolean) => {
    if (next) {
      const id = driveId();
      if (!id) return;
      await props.addOneDrive(id);
      showStatus('OneDrive activé', 'success');
    } else {
      const current = mount();
      if (!current) return;
      await props.removeWithConfirm(current.id);
      showStatus('OneDrive désactivé', 'success');
    }
  };

  return (
    <MountCard
      kind="onedrive"
      name="OneDrive"
      mountPoint={mount()?.mount_point ?? null}
      isMounted={!!mount()}
      onToggle={onToggle}
    />
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
