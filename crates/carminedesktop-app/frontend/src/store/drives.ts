// Drives store — one reactive record per mounted drive, keyed by driveId.
// Solid's `createStore` gives nested reactivity: a `drive:status` event that
// only changes `syncState` re-runs only the effects that read that field,
// leaving the name, online flag and upload counters untouched on the DOM.

import { createStore, produce, reconcile } from 'solid-js/store';

import { api } from '../ipc';
import {
  onDriveOnline,
  onDriveStatus,
  onDriveUploadProgress,
} from '../eventBus';
import type {
  DashboardStatus,
  DriveStatus,
  DriveSyncState,
  UploadQueueInfo,
} from '../bindings';

export interface DriveView {
  driveId: string;
  name: string;
  mountPoint: string;
  online: boolean;
  lastSynced: string | null;
  syncState: DriveSyncState;
  uploadQueue: UploadQueueInfo;
}

export interface DrivesState {
  loaded: boolean;
  list: DriveView[];
}

const [state, setState] = createStore<DrivesState>({ loaded: false, list: [] });
export const drives = state;

function toView(d: DriveStatus): DriveView {
  return {
    driveId: d.driveId,
    name: d.name,
    mountPoint: d.mountPoint,
    online: d.online,
    lastSynced: d.lastSynced,
    syncState: d.syncState,
    uploadQueue: { ...d.uploadQueue },
  };
}

export function ingestDashboardDrives(ds: DashboardStatus): void {
  const list = ds.drives.map(toView);
  setState('list', reconcile(list, { key: 'driveId', merge: true }));
  setState('loaded', true);
}

export async function bootstrapDrives(): Promise<DashboardStatus> {
  const ds = await api.getDashboardStatus();
  ingestDashboardDrives(ds);
  return ds;
}

export function setDriveSync(driveId: string, syncState: DriveSyncState): void {
  setState(
    produce((s) => {
      const d = s.list.find((x) => x.driveId === driveId);
      if (d && d.syncState !== syncState) d.syncState = syncState;
    }),
  );
}

export function setDriveOnline(driveId: string, online: boolean): void {
  setState(
    produce((s) => {
      const d = s.list.find((x) => x.driveId === driveId);
      if (d && d.online !== online) d.online = online;
    }),
  );
}

export function setDriveUploadQueue(
  driveId: string,
  q: Pick<
    UploadQueueInfo,
    'queueDepth' | 'inFlight' | 'failedCount' | 'totalUploaded' | 'totalFailed'
  >,
): void {
  setState(
    produce((s) => {
      const d = s.list.find((x) => x.driveId === driveId);
      if (!d) return;
      const cur = d.uploadQueue;
      if (cur.queueDepth !== q.queueDepth) cur.queueDepth = q.queueDepth;
      if (cur.inFlight !== q.inFlight) cur.inFlight = q.inFlight;
      if (cur.failedCount !== q.failedCount) cur.failedCount = q.failedCount;
      if (cur.totalUploaded !== q.totalUploaded) cur.totalUploaded = q.totalUploaded;
      if (cur.totalFailed !== q.totalFailed) cur.totalFailed = q.totalFailed;
    }),
  );
}

export interface UploadAggregate {
  inFlight: number;
  queued: number;
}

export function aggregateUploadQueue(list: DriveView[]): UploadAggregate {
  let inFlight = 0;
  let queued = 0;
  for (const d of list) {
    inFlight += d.uploadQueue.inFlight;
    queued += d.uploadQueue.queueDepth;
  }
  return { inFlight, queued };
}

let wired = false;
export function attachDriveEvents(): void {
  if (wired) return;
  wired = true;
  void onDriveStatus((e) => {
    setDriveSync(e.driveId, e.state);
    // When a sync cycle completes we refresh metadata (lastSynced) — cheap,
    // one shot per `error|up_to_date` transition, no polling interval.
    if (e.state !== 'syncing') void refreshLastSynced();
  });
  void onDriveOnline((e) => setDriveOnline(e.driveId, e.online));
  void onDriveUploadProgress((e) => {
    setDriveUploadQueue(e.driveId, {
      queueDepth: e.queueDepth,
      inFlight: e.inFlight,
      failedCount: e.failedCount,
      totalUploaded: e.totalUploaded,
      totalFailed: e.totalFailed,
    });
  });
}

async function refreshLastSynced(): Promise<void> {
  try {
    const ds = await api.getDashboardStatus();
    setState(
      produce((s) => {
        for (const fresh of ds.drives) {
          const d = s.list.find((x) => x.driveId === fresh.driveId);
          if (d && d.lastSynced !== fresh.lastSynced) d.lastSynced = fresh.lastSynced;
        }
      }),
    );
  } catch {
    // Non-fatal — the live events still reflect the sync state.
  }
}
