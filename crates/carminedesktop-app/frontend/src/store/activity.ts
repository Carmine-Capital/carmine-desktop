// Activity feed — ring buffer (cap 500) newest first. Bootstraps from the
// Rust ring via `get_activity_feed` (returned oldest first, reversed here)
// and grows push-driven from `activity:append` events.

import { createMemo } from 'solid-js';
import { createStore, produce } from 'solid-js/store';

import { api } from '../ipc';
import { onActivityAppend } from '../eventBus';
import type { ActivityEntry, ActivityKind } from '../bindings';

const CAP = 500;

export type ActivityFilter = 'all' | 'local' | 'remote' | 'deleted' | 'conflict';

export interface ActivityState {
  loaded: boolean;
  entries: ActivityEntry[];
  expanded: boolean;
  filter: ActivityFilter;
}

const [state, setState] = createStore<ActivityState>({
  loaded: false,
  entries: [],
  expanded: false,
  filter: 'all',
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

export function setActivityFilter(filter: ActivityFilter): void {
  if (state.filter === filter) return;
  setState('filter', filter);
}

let wired = false;
export function attachActivityEvents(): void {
  if (wired) return;
  wired = true;
  void onActivityAppend(appendActivity);
}

function kindOp(kind: ActivityKind): string {
  return kind.op;
}

function matchesFilter(entry: ActivityEntry, filter: ActivityFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'local':
      return entry.source === 'local';
    case 'remote':
      return entry.source === 'remote';
    case 'deleted':
      return kindOp(entry.kind) === 'deleted';
    case 'conflict':
      return kindOp(entry.kind) === 'conflict';
  }
}

/// One displayable row: either a single entry or a collapsed group header.
/// Group rows carry only the `groupId`; the actual member entries are resolved
/// inside `GroupRow` via a memo over `activity.entries`. This keeps the
/// wrapper reference stable when new members join the group so Solid's `<For>`
/// does not remount the whole group row (which would re-trigger autoAnimate
/// and flicker).
export type ActivityRow =
  | { kind: 'entry'; entry: ActivityEntry }
  | { kind: 'group'; groupId: string };

/// Row wrappers are cached so `activityRows()` returns the SAME object
/// reference for an entry/group across memo re-runs. Solid's `<For>` is
/// keyed by reference: fresh wrappers on every tick would remount every
/// row and cause the list to flicker on every append.
const entryRowCache = new WeakMap<ActivityEntry, Extract<ActivityRow, { kind: 'entry' }>>();
const groupRowCache = new Map<string, Extract<ActivityRow, { kind: 'group' }>>();

function entryRow(entry: ActivityEntry): ActivityRow {
  let row = entryRowCache.get(entry);
  if (!row) {
    row = { kind: 'entry', entry };
    entryRowCache.set(entry, row);
  }
  return row;
}

function groupRow(groupId: string): ActivityRow {
  let row = groupRowCache.get(groupId);
  if (!row) {
    row = { kind: 'group', groupId };
    groupRowCache.set(groupId, row);
  }
  return row;
}

/// Collapse consecutive entries sharing the same non-null `groupId` into a
/// single group row. Entries with `groupId === null` or with only one member
/// in the current list stay as standalone rows.
function collapseGroups(entries: ActivityEntry[]): ActivityRow[] {
  const rows: ActivityRow[] = [];
  const seenGroups = new Set<string>();
  let i = 0;
  while (i < entries.length) {
    const cur = entries[i];
    if (!cur) break;
    if (cur.groupId) {
      const groupId = cur.groupId;
      let count = 1;
      let j = i + 1;
      while (j < entries.length) {
        const next = entries[j];
        if (!next || next.groupId !== groupId) break;
        count++;
        j++;
      }
      if (count === 1) {
        rows.push(entryRow(cur));
      } else {
        rows.push(groupRow(groupId));
        seenGroups.add(groupId);
      }
      i = j;
    } else {
      rows.push(entryRow(cur));
      i++;
    }
  }
  for (const key of groupRowCache.keys()) {
    if (!seenGroups.has(key)) groupRowCache.delete(key);
  }
  return rows;
}

/// Derived, memoised view of the feed after filter + grouping.
export const activityRows = createMemo<ActivityRow[]>(() => {
  const list = activity.entries.filter((e) => matchesFilter(e, activity.filter));
  return collapseGroups(list);
});

/// Resolve the members of a group from the current store. Used by `GroupRow`
/// so the row can stay mounted across appends while the count / child list
/// updates reactively.
export function entriesForGroup(groupId: string): ActivityEntry[] {
  return activity.entries.filter((e) => e.groupId === groupId);
}
