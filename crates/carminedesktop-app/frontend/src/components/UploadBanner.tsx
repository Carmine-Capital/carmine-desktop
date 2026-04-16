import { For, Show, createMemo, type JSX } from 'solid-js';

import { aggregateUploadQueue, drives } from '../store/drives';
import { cache, setWritebackExpanded } from '../store/cache';
import { autoAnimateList } from './autoAnimate';

/** Aggregated upload banner — sums queueDepth + inFlight across every mounted
 *  drive and toggles a detail list of pending file names (fetched from the
 *  writeback queue snapshot in `cache` store). */
export const UploadBanner = (): JSX.Element => {
  const agg = createMemo(() => aggregateUploadQueue(drives.list));
  const hasActivity = () => agg().inFlight + agg().queued > 0;
  const summary = createMemo(() => {
    const { inFlight, queued } = agg();
    const parts: string[] = [];
    if (inFlight > 0) parts.push(`${inFlight} en envoi`);
    if (queued > 0) parts.push(`${queued} en attente`);
    return parts.join(', ');
  });

  const toggle = () => setWritebackExpanded(!cache.writebackExpanded);

  return (
    <Show when={hasActivity()}>
      <div
        class="upload-summary"
        role="button"
        tabindex="0"
        onClick={toggle}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            toggle();
          }
        }}
      >
        <span class={`disclosure-arrow${cache.writebackExpanded ? ' expanded' : ''}`}>
          ▶
        </span>
        <span class="upload-summary-text">{summary()}</span>
      </div>
      <Show when={cache.writebackExpanded && cache.writebackQueue.length > 0}>
        <div class="upload-detail" ref={autoAnimateList}>
          <For each={cache.writebackQueue}>
            {(entry) => (
              <div class="upload-detail-file">
                {entry.fileName ?? entry.itemId}
              </div>
            )}
          </For>
        </div>
      </Show>
    </Show>
  );
};
