import { Show, createMemo, type JSX } from 'solid-js';

import { cache } from '../store/cache';
import { pins } from '../store/pins';
import { formatBytes } from '../utils/format';

function barColor(pct: number): string {
  if (pct >= 90) return 'red';
  if (pct >= 70) return 'amber';
  return 'green';
}

export const CacheSection = (): JSX.Element => {
  const pct = createMemo(() => {
    const max = cache.diskMaxBytes;
    if (max <= 0) return 0;
    return Math.min((cache.diskUsedBytes / max) * 100, 100);
  });
  const fillClass = createMemo(() => `cache-bar-fill ${barColor(pct())}`);
  const usage = createMemo(
    () => `${formatBytes(cache.diskUsedBytes)} / ${formatBytes(cache.diskMaxBytes)}`,
  );
  const pinCounts = createMemo(() => {
    const counts = { downloaded: 0, partial: 0, stale: 0 };
    for (const p of pins.list) counts[p.status] = (counts[p.status] ?? 0) + 1;
    return counts;
  });
  const pinSummary = createMemo(() => {
    const total = pins.list.length;
    if (total === 0) return '';
    const { downloaded, partial, stale } = pinCounts();
    const parts: string[] = [];
    if (downloaded > 0) parts.push(`${downloaded} Disponibles`);
    if (partial > 0) parts.push(`${partial} Partiels`);
    if (stale > 0) parts.push(`${stale} Obsolètes`);
    return `${total} dossiers épinglés · ${parts.join(', ')}`;
  });

  return (
    <div>
      <div class="cache-bar">
        <div
          class={fillClass()}
          style={{ width: `${pct().toFixed(1)}%` }}
        />
      </div>
      <div class="cache-text">{usage()}</div>
      <Show
        when={pins.list.length > 0}
        fallback={<div class="pin-summary-empty">Aucun dossier hors-ligne</div>}
      >
        <div class="pin-summary">{pinSummary()}</div>
      </Show>
    </div>
  );
};
