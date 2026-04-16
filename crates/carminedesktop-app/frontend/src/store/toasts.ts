// Toast store — transient notifications rendered by `ToastHost`.  Older entries
// fade out when more than three are visible; imperative API mirrors the shape
// of the legacy `showStatus` helper but returns the toast id so callers may
// dismiss programmatically (e.g. after a retry succeeds).

import { createStore, produce } from 'solid-js/store';

export type ToastKind = 'success' | 'error' | 'info' | 'warning';

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
}

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  message?: string;
  action?: ToastAction;
  createdAt: number;
}

export interface ToastsState {
  list: Toast[];
}

const [state, setState] = createStore<ToastsState>({ list: [] });

export const toasts = state;

let seq = 0;

export interface PushToastArgs {
  kind: ToastKind;
  title: string;
  message?: string;
  action?: ToastAction;
}

/** Append a toast and return its id.  Callers can dismiss the toast early via
 *  `dismissToast(id)` — useful when an action's follow-up toast should replace
 *  the initial one. */
export function pushToast(args: PushToastArgs): number {
  seq += 1;
  const toast: Toast = {
    id: seq,
    kind: args.kind,
    title: args.title,
    message: args.message,
    action: args.action,
    createdAt: Date.now(),
  };
  setState(
    produce((s) => {
      s.list.push(toast);
    }),
  );
  return toast.id;
}

export function dismissToast(id: number): void {
  setState('list', (l) => l.filter((t) => t.id !== id));
}

export function clearToasts(): void {
  setState('list', []);
}
