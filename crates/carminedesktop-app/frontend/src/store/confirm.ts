// Imperative confirm dialog — a single outstanding request drives the global
// `ConfirmModal`.  Callers `await confirm({...})` and the promise resolves
// with the user's choice.  A second call before the first resolves will
// auto-cancel the pending one so we never stack modals.

import { createSignal } from 'solid-js';

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

export interface ConfirmRequest {
  id: number;
  options: ConfirmOptions;
  resolve: (value: boolean) => void;
}

const [current, setCurrent] = createSignal<ConfirmRequest | null>(null);

export const confirmRequest = current;

let seq = 0;

/** Open the confirm dialog.  Resolves `true` on confirm, `false` on cancel /
 *  overlay click / Esc.  If another confirm is already in flight it resolves
 *  to `false` so callers never deadlock. */
export function confirm(options: ConfirmOptions): Promise<boolean> {
  const active = current();
  if (active) {
    active.resolve(false);
  }
  seq += 1;
  const id = seq;
  return new Promise<boolean>((resolve) => {
    setCurrent({ id, options, resolve });
  });
}

/** Resolve the current request with the given choice and clear state. */
export function resolveConfirm(choice: boolean): void {
  const active = current();
  if (!active) return;
  setCurrent(null);
  active.resolve(choice);
}
