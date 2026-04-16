import { createMemo, type JSX } from 'solid-js';

import type { DriveView } from '../store/drives';
import { formatRelativeTime } from '../utils/format';

function statusKey(d: DriveView): string {
  if (!d.online) return 'offline';
  if (d.syncState === 'error') return 'error';
  if (d.syncState === 'syncing') return 'syncing';
  return 'ok';
}

function syncText(d: DriveView): string {
  if (!d.online) return 'Hors-ligne';
  if (d.syncState === 'error') return 'Erreur';
  if (d.syncState === 'syncing') {
    const total = d.uploadQueue.inFlight + d.uploadQueue.queueDepth;
    return total > 0 ? `Synchro ${total} fichiers` : 'Synchro en cours';
  }
  return 'À jour';
}

/** One drive's status card.  Each field read is a separate reactive
 *  subscription — a `drive:status` flipping `syncState` only re-runs the
 *  status label/dot effects, not the name or last-sync nodes. */
export const DriveCard = (props: { drive: DriveView }): JSX.Element => {
  const dotClass = createMemo(() => `status-dot ${statusKey(props.drive)}`);
  const status = createMemo(() => syncText(props.drive));
  const lastSync = createMemo(
    () => `Dernière synchro : ${formatRelativeTime(props.drive.lastSynced)}`,
  );

  return (
    <div class="drive-card">
      <div class="drive-card-header">
        <span class={dotClass()} />
        <div class="drive-card-name">{props.drive.name}</div>
      </div>
      <div class="drive-card-status">{status()}</div>
      <div class="drive-card-last-sync">{lastSync()}</div>
    </div>
  );
};
