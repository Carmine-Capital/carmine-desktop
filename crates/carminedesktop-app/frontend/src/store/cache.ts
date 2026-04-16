// Cache store — disk usage + writeback queue for the dashboard "Cache &
// Hors-ligne" section.  Pin counts come from the pins store, so we only hold
// the disk/writeback slice here.  Re-fetched on demand (no live event today —
// writeback drain is observable via drive:upload-progress).

import { createStore, produce, reconcile } from 'solid-js/store';

import { api } from '../ipc';
import type { CacheStatsResponse, WritebackEntry } from '../bindings';

export interface CacheState {
  loaded: boolean;
  diskUsedBytes: number;
  diskMaxBytes: number;
  writebackQueue: WritebackEntry[];
  writebackExpanded: boolean;
}

const [state, setState] = createStore<CacheState>({
  loaded: false,
  diskUsedBytes: 0,
  diskMaxBytes: 0,
  writebackQueue: [],
  writebackExpanded: false,
});

export const cache = state;

function writebackKey(e: WritebackEntry, i: number): string {
  return `${e.driveId}:${e.itemId}:${i}`;
}

export function ingestCacheStats(stats: CacheStatsResponse): void {
  const keyed = stats.writebackQueue.map((entry, i) => ({
    ...entry,
    _key: writebackKey(entry, i),
  }));
  setState(
    produce((s) => {
      if (s.diskUsedBytes !== stats.diskUsedBytes) s.diskUsedBytes = stats.diskUsedBytes;
      if (s.diskMaxBytes !== stats.diskMaxBytes) s.diskMaxBytes = stats.diskMaxBytes;
      s.loaded = true;
    }),
  );
  setState(
    'writebackQueue',
    reconcile(
      keyed.map(({ _key, ...rest }) => rest),
      { key: 'itemId', merge: true },
    ),
  );
}

export async function bootstrapCache(): Promise<CacheStatsResponse> {
  const stats = await api.getCacheStats();
  ingestCacheStats(stats);
  return stats;
}

export function setWritebackExpanded(expanded: boolean): void {
  if (state.writebackExpanded === expanded) return;
  setState('writebackExpanded', expanded);
}
