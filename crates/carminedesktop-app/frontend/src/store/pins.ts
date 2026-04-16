// Pin store — one ordered list of `PinView` records keyed by `${driveId}:${itemId}`.
// Solid's `createStore` gives us nested reactivity: mutating a single field on a
// record re-runs only the effects that read that field, so per-pin updates never
// touch sibling rows. List order changes (add/remove) animate via auto-animate.

import { createStore, produce, reconcile } from 'solid-js/store';

import { api } from '../ipc';
import { onPinHealth, onPinRemoved } from '../eventBus';
import type { PinHealthEvent } from '../bindings';

export type PinStatus = 'downloaded' | 'partial' | 'stale' | 'analyzing' | 'expired' | 'unknown';

export interface PinView {
  key: string;
  driveId: string;
  itemId: string;
  folderName: string;
  mountName: string;
  status: 'downloaded' | 'partial' | 'stale';
  totalFiles: number;
  cachedFiles: number;
  pinnedAt: string;
  expiresAt: string;
}

export interface PinsState {
  loaded: boolean;
  list: PinView[];
}

export const pinKey = (driveId: string, itemId: string) => `${driveId}:${itemId}`;

const [pinsState, setPinsState] = createStore<PinsState>({ loaded: false, list: [] });

export const pins = pinsState;

/** Bootstrap from `list_offline_pins` + `get_cache_stats`.  Merges health metrics
 *  (totalFiles/cachedFiles/status) into the offline-pin list.  Safe to call
 *  multiple times — `reconcile` preserves identity of unchanged records. */
export async function bootstrapPins(): Promise<void> {
  const [offline, stats] = await Promise.all([
    api.listOfflinePins(),
    api.getCacheStats(),
  ]);
  const healthByKey = new Map(
    stats.pinnedItems.map((h) => [pinKey(h.driveId, h.itemId), h] as const),
  );
  const list: PinView[] = offline.map((p) => {
    const key = pinKey(p.drive_id, p.item_id);
    const h = healthByKey.get(key);
    return {
      key,
      driveId: p.drive_id,
      itemId: p.item_id,
      folderName: p.folder_name,
      mountName: p.mount_name,
      status: h?.status ?? 'partial',
      totalFiles: h?.totalFiles ?? 0,
      cachedFiles: h?.cachedFiles ?? 0,
      pinnedAt: p.pinned_at,
      expiresAt: p.expires_at,
    };
  });
  setPinsState('list', reconcile(list, { key: 'key', merge: true }));
  setPinsState('loaded', true);
}

/** Upsert a pin from a `pin:health` event.  Mutates only the touched fields
 *  when the pin already exists — Solid's fine-grained reactivity then updates
 *  only the bound nodes (count, progress, health pill) without re-rendering
 *  siblings or re-running transitions on unchanged classes. */
export function upsertPinHealth(ev: PinHealthEvent): void {
  const key = pinKey(ev.driveId, ev.itemId);
  setPinsState(
    produce((s) => {
      const idx = s.list.findIndex((p) => p.key === key);
      if (idx === -1) {
        s.list.push({
          key,
          driveId: ev.driveId,
          itemId: ev.itemId,
          folderName: ev.folderName,
          mountName: ev.mountName,
          status: ev.status,
          totalFiles: ev.totalFiles,
          cachedFiles: ev.cachedFiles,
          pinnedAt: ev.pinnedAt,
          expiresAt: ev.expiresAt,
        });
        return;
      }
      const p = s.list[idx]!;
      if (p.folderName !== ev.folderName) p.folderName = ev.folderName;
      if (p.mountName !== ev.mountName) p.mountName = ev.mountName;
      if (p.status !== ev.status) p.status = ev.status;
      if (p.totalFiles !== ev.totalFiles) p.totalFiles = ev.totalFiles;
      if (p.cachedFiles !== ev.cachedFiles) p.cachedFiles = ev.cachedFiles;
      if (p.pinnedAt !== ev.pinnedAt) p.pinnedAt = ev.pinnedAt;
      if (p.expiresAt !== ev.expiresAt) p.expiresAt = ev.expiresAt;
    }),
  );
}

export function removePin(driveId: string, itemId: string): void {
  const key = pinKey(driveId, itemId);
  setPinsState('list', (l) => l.filter((p) => p.key !== key));
}

let wired = false;
/** Subscribe to the Rust push stream once for the lifetime of the app. */
export function attachPinEvents(): void {
  if (wired) return;
  wired = true;
  void onPinHealth(upsertPinHealth);
  void onPinRemoved((e) => removePin(e.driveId, e.itemId));
}
