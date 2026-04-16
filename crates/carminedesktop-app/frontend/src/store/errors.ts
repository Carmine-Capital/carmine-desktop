// Dashboard errors ring — bootstraps from `get_recent_errors` (oldest first,
// reversed here to newest first) and grows push-driven from `error:append`.

import { createStore, produce } from 'solid-js/store';

import { api } from '../ipc';
import { onErrorAppend } from '../eventBus';
import type { DashboardError } from '../bindings';

const CAP = 100;

export interface ErrorsState {
  loaded: boolean;
  entries: DashboardError[];
}

const [state, setState] = createStore<ErrorsState>({ loaded: false, entries: [] });

export const errors = state;

export async function bootstrapErrors(): Promise<void> {
  const list = await api.getRecentErrors();
  const reversed = [...list].reverse().slice(0, CAP);
  setState({ loaded: true, entries: reversed });
}

export function appendError(entry: DashboardError): void {
  setState(
    produce((s) => {
      s.entries.unshift(entry);
      if (s.entries.length > CAP) s.entries.length = CAP;
    }),
  );
}

/** Client-side dismissal — drops a single entry from the ring without touching
 *  the backend.  Matches on `(driveId, timestamp, fileName)` since
 *  `DashboardError` has no stable id; the combination is unique in practice
 *  because the ring is append-only and timestamps are millisecond-resolution. */
export function dismissError(entry: DashboardError): void {
  setState(
    produce((s) => {
      const idx = s.entries.findIndex(
        (e) =>
          e.timestamp === entry.timestamp &&
          e.driveId === entry.driveId &&
          e.fileName === entry.fileName &&
          e.errorType === entry.errorType,
      );
      if (idx >= 0) s.entries.splice(idx, 1);
    }),
  );
}

let wired = false;
export function attachErrorEvents(): void {
  if (wired) return;
  wired = true;
  void onErrorAppend(appendError);
}
