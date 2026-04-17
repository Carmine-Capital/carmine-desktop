// Cache store — disk usage for the dashboard "Cache & Hors-ligne" section.
// Pin counts come from the pins store, so we only hold the disk slice here.
// Re-fetched on demand.

import { createStore, produce } from 'solid-js/store';

import { api } from '../ipc';
import type { CacheStatsResponse } from '../bindings';

export interface CacheState {
  loaded: boolean;
  diskUsedBytes: number;
  diskMaxBytes: number;
}

const [state, setState] = createStore<CacheState>({
  loaded: false,
  diskUsedBytes: 0,
  diskMaxBytes: 0,
});

export const cache = state;

export function ingestCacheStats(stats: CacheStatsResponse): void {
  setState(
    produce((s) => {
      if (s.diskUsedBytes !== stats.diskUsedBytes) s.diskUsedBytes = stats.diskUsedBytes;
      if (s.diskMaxBytes !== stats.diskMaxBytes) s.diskMaxBytes = stats.diskMaxBytes;
      s.loaded = true;
    }),
  );
}

export async function bootstrapCache(): Promise<CacheStatsResponse> {
  const stats = await api.getCacheStats();
  ingestCacheStats(stats);
  return stats;
}
