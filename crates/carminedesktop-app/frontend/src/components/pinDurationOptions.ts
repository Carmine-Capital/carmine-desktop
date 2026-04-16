// Shared default set of pin-duration options used by both the Offline panel
// (global default TTL) and the per-pin picker on `PinCard`.  Keeping them in
// one place means the UX stays consistent and the labels stay in French.
//
// `Jamais` maps to a sentinel that the backend treats as "max TTL" — a very
// large number of seconds (~100 years) so the pin effectively never expires
// without requiring a separate nullable code path.

import type { PinDurationOption } from './PinDurationPicker';

const DAY = 24 * 3600;
export const NEVER_EXPIRE_SECS = 100 * 365 * DAY;

export const DEFAULT_PIN_DURATION_OPTIONS: PinDurationOption[] = [
  { label: '1j', secs: DAY },
  { label: '7j', secs: 7 * DAY },
  { label: '30j', secs: 30 * DAY },
  { label: 'Jamais', secs: NEVER_EXPIRE_SECS },
];
