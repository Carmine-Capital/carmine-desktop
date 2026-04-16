// Mounts store — mounts, SharePoint libraries and OneDrive drive info.
// `mounts` is kept as a reconciled list so individual MountCards only re-render
// when their own fields change.  `libraries` uses a small state machine
// (idle → loading → ready|error) so the panel can show the right empty state
// without flicker between bootstrap and live updates.

import { createStore, produce, reconcile } from 'solid-js/store';

import { api, type AddMountArgs } from '../ipc';
import type { DriveInfo, MountInfo, PrimarySiteInfo } from '../bindings';

export type LibrariesStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface MountsState {
  loaded: boolean;
  mounts: MountInfo[];
  libraries: DriveInfo[];
  librariesStatus: LibrariesStatus;
  librariesError: string | null;
  driveInfo: DriveInfo | null;
  primarySite: PrimarySiteInfo | null;
}

const [state, setState] = createStore<MountsState>({
  loaded: false,
  mounts: [],
  libraries: [],
  librariesStatus: 'idle',
  librariesError: null,
  driveInfo: null,
  primarySite: null,
});

export const mounts = state;

function setMountsList(list: MountInfo[]): void {
  setState('mounts', reconcile(list, { key: 'id', merge: true }));
}

/** Refresh `list_mounts`.  Called after add/remove round-trips so the UI reflects
 *  the canonical backend state once the write has landed. */
export async function refreshMounts(): Promise<void> {
  const list = await api.listMounts();
  setMountsList(list);
  setState('loaded', true);
}

/** Load OneDrive info + primary SharePoint site + library list in parallel.
 *  Each sub-call has its own error path: failure to fetch libraries surfaces a
 *  friendly error without clobbering the OneDrive card, and vice versa. */
export async function loadLibraries(): Promise<void> {
  setState(
    produce((s) => {
      s.librariesStatus = 'loading';
      s.librariesError = null;
    }),
  );
  try {
    const [libraries, driveInfo, siteInfo] = await Promise.all([
      api.listPrimarySiteLibraries().catch(() => [] as DriveInfo[]),
      api.getDriveInfo().catch(() => null),
      api.getPrimarySiteInfo().catch(() => null),
    ]);
    setState(
      produce((s) => {
        s.libraries = libraries;
        s.driveInfo = driveInfo;
        s.primarySite = siteInfo;
        s.librariesStatus = 'ready';
      }),
    );
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    setState(
      produce((s) => {
        s.librariesStatus = 'error';
        s.librariesError = msg;
      }),
    );
  }
}

export async function bootstrapMounts(): Promise<void> {
  await refreshMounts();
}

/** Add a mount and refresh the list.  On failure we still refresh so the UI
 *  recovers from any partial state on the backend side. */
export async function addMount(args: AddMountArgs): Promise<void> {
  try {
    await api.addMount(args);
  } finally {
    await refreshMounts().catch(() => { /* list stays as-is */ });
  }
}

export async function removeMount(id: string): Promise<void> {
  try {
    await api.removeMount(id);
  } finally {
    await refreshMounts().catch(() => { /* list stays as-is */ });
  }
}

/** Convenience lookup for the Mounts panel — returns the MountInfo matching a
 *  given drive_id (if any), so a SharePoint library knows whether it's
 *  currently mounted. */
export function mountForDrive(driveId: string): MountInfo | undefined {
  return state.mounts.find((m) => m.drive_id === driveId);
}

export function oneDriveMount(): MountInfo | undefined {
  return state.mounts.find((m) => m.mount_type === 'drive');
}
