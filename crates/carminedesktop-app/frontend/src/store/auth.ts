// Auth degradation signal — driven by `auth:state` push events and by the
// auth_degraded flag on `get_dashboard_status` at bootstrap.  Kept as a plain
// signal (no object wrapping) so <Show> subscribes to the single boolean.

import { createSignal } from 'solid-js';

import { onAuthState } from '../eventBus';
import type { DashboardStatus } from '../bindings';

const [degraded, setDegraded] = createSignal(false);
export const authDegraded = degraded;

export function ingestDashboardAuth(ds: DashboardStatus): void {
  setDegraded(!!ds.authDegraded);
}

let wired = false;
export function attachAuthEvents(): void {
  if (wired) return;
  wired = true;
  void onAuthState((e) => setDegraded(!!e.degraded));
}
