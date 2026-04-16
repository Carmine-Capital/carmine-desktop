// Activity feed — ring buffer (cap 100) newest first.  Bootstraps from the
// Rust ring via `get_activity_feed` (returned oldest first, reversed here)
// and grows push-driven from `activity:append` events.

import { createStore, produce } from 'solid-js/store';

import { api } from '../ipc';
import { onActivityAppend } from '../eventBus';
import type { ActivityEntry } from '../bindings';

const CAP = 100;

export interface ActivityState {
  loaded: boolean;
  entries: ActivityEntry[];
  expanded: boolean;
}

const [state, setState] = createStore<ActivityState>({
  loaded: false,
  entries: [],
  expanded: false,
});

export const activity = state;

export async function bootstrapActivity(): Promise<void> {
  const feed = await api.getActivityFeed();
  const reversed = [...feed].reverse().slice(0, CAP);
  setState({ loaded: true, entries: reversed });
}

export function appendActivity(entry: ActivityEntry): void {
  setState(
    produce((s) => {
      s.entries.unshift(entry);
      if (s.entries.length > CAP) s.entries.length = CAP;
    }),
  );
}

export function setActivityExpanded(expanded: boolean): void {
  if (state.expanded === expanded) return;
  setState('expanded', expanded);
}

let wired = false;
export function attachActivityEvents(): void {
  if (wired) return;
  wired = true;
  void onActivityAppend(appendActivity);
}
