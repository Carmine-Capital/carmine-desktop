import { For, Match, Show, Switch, createMemo, createSignal, type JSX } from 'solid-js';

import type { ActivityEntry, ActivityKind, ActivitySource } from '../bindings';
import {
  activity,
  activityRows,
  entriesForGroup,
  setActivityExpanded,
  setActivityFilter,
  type ActivityFilter,
  type ActivityRow,
} from '../store/activity';
import { formatRelativeTime, truncatePath } from '../utils/format';
import { autoAnimateList } from './autoAnimate';

const COLLAPSED_LIMIT = 10;

const FILTER_CHIPS: ReadonlyArray<{ id: ActivityFilter; label: string }> = [
  { id: 'all', label: 'Tout' },
  { id: 'local', label: 'Locaux' },
  { id: 'remote', label: 'Distants' },
  { id: 'deleted', label: 'Suppressions' },
  { id: 'conflict', label: 'Conflits' },
];

const KIND_LABEL: Record<ActivityKind['op'], string> = {
  created: 'créé',
  modified: 'modifié',
  deleted: 'supprimé',
  renamed: 'renommé',
  moved: 'déplacé',
  conflict: 'conflit',
  pinned: 'épinglé',
  unpinned: 'désépinglé',
};

const SOURCE_ICON: Record<ActivitySource, string> = {
  local: '↑',
  remote: '↓',
  system: '⚙',
};

const SOURCE_LABEL: Record<ActivitySource, string> = {
  local: 'Local',
  remote: 'Distant',
  system: 'Système',
};

function kindTagClass(kind: ActivityKind): string {
  return `activity-tag activity-tag-${kind.op}`;
}

function kindDetail(kind: ActivityKind): string | null {
  if (kind.op === 'renamed') return `depuis ${kind.from}`;
  if (kind.op === 'moved') return `depuis ${kind.from}`;
  if (kind.op === 'conflict') return `copie : ${kind.conflictName}`;
  return null;
}

const EntryRow = (props: { entry: ActivityEntry }): JSX.Element => {
  const tagClass = createMemo(() => kindTagClass(props.entry.kind));
  const tagLabel = createMemo(() => KIND_LABEL[props.entry.kind.op]);
  const sourceIcon = createMemo(() => SOURCE_ICON[props.entry.source]);
  const sourceLabel = createMemo(() => SOURCE_LABEL[props.entry.source]);
  const name = createMemo(() => truncatePath(props.entry.filePath));
  const time = createMemo(() => formatRelativeTime(props.entry.timestamp));
  const detail = createMemo(() => kindDetail(props.entry.kind));
  return (
    <li class="activity-row">
      <span class={`activity-source activity-source-${props.entry.source}`} title={sourceLabel()}>
        {sourceIcon()}
      </span>
      <span class={tagClass()}>{tagLabel()}</span>
      <span class="activity-name">{name()}</span>
      <Show when={detail()}>{(d) => <span class="activity-detail">{d()}</span>}</Show>
      <span class="activity-time">{time()}</span>
    </li>
  );
};

const GroupRow = (props: { groupId: string }): JSX.Element => {
  const [open, setOpen] = createSignal(false);
  const entries = createMemo(() => entriesForGroup(props.groupId));
  const lead = createMemo<ActivityEntry | null>(() => entries()[0] ?? null);
  const count = createMemo(() => entries().length);
  const tagClass = createMemo(() => {
    const l = lead();
    return l ? kindTagClass(l.kind) : 'activity-tag';
  });
  const tagLabel = createMemo(() => {
    const l = lead();
    return l ? KIND_LABEL[l.kind.op] : '';
  });
  const sourceIcon = createMemo(() => {
    const l = lead();
    return l ? SOURCE_ICON[l.source] : '';
  });
  const sourceLabel = createMemo(() => {
    const l = lead();
    return l ? SOURCE_LABEL[l.source] : '';
  });
  const sourceClass = createMemo(() => {
    const l = lead();
    return l ? `activity-source activity-source-${l.source}` : 'activity-source';
  });
  const parentPath = createMemo(() => {
    const l = lead();
    if (!l) return '/';
    const p = l.filePath;
    const idx = p.lastIndexOf('/');
    return idx <= 0 ? '/' : p.slice(0, idx);
  });
  const time = createMemo(() => {
    const l = lead();
    return l ? formatRelativeTime(l.timestamp) : '';
  });
  return (
    <Show when={count() > 0}>
      <li class="activity-row activity-row-group">
        <button
          type="button"
          class="activity-group-toggle"
          aria-expanded={open()}
          onClick={() => setOpen(!open())}
          title={open() ? 'Replier le groupe' : 'Déplier le groupe'}
        >
          {open() ? '▾' : '▸'}
        </button>
        <span class={sourceClass()} title={sourceLabel()}>
          {sourceIcon()}
        </span>
        <span class={tagClass()}>{tagLabel()}</span>
        <span class="activity-name">
          {truncatePath(parentPath())} — <strong>{count()}</strong> éléments
        </span>
        <span class="activity-time">{time()}</span>
      </li>
      <Show when={open()}>
        <For each={entries()}>
          {(entry) => (
            <li class="activity-row activity-row-grouped-child">
              <span class="activity-source-spacer" />
              <span class="activity-name">↳ {truncatePath(entry.filePath)}</span>
              <span class="activity-time">{formatRelativeTime(entry.timestamp)}</span>
            </li>
          )}
        </For>
      </Show>
    </Show>
  );
};

const Row = (props: { row: ActivityRow }): JSX.Element => {
  return (
    <Switch>
      <Match when={props.row.kind === 'entry' ? props.row : null}>
        {(r) => <EntryRow entry={(r() as { entry: ActivityEntry }).entry} />}
      </Match>
      <Match when={props.row.kind === 'group' ? props.row : null}>
        {(r) => <GroupRow groupId={(r() as { groupId: string }).groupId} />}
      </Match>
    </Switch>
  );
};

export const ActivityFeed = (): JSX.Element => {
  const visibleRows = createMemo(() => {
    const rows = activityRows();
    if (activity.expanded) return rows;
    return rows.slice(0, COLLAPSED_LIMIT);
  });
  const hasMore = () => activityRows().length > COLLAPSED_LIMIT;
  const isEmpty = () => activityRows().length === 0;

  return (
    <div class="activity-panel">
      <ul class="activity-filter-chips" role="tablist">
        <For each={FILTER_CHIPS}>
          {(chip) => (
            <li>
              <button
                type="button"
                class={`activity-chip ${activity.filter === chip.id ? 'is-active' : ''}`}
                role="tab"
                aria-selected={activity.filter === chip.id}
                onClick={() => setActivityFilter(chip.id)}
              >
                {chip.label}
              </button>
            </li>
          )}
        </For>
      </ul>
      <Show
        when={!isEmpty()}
        fallback={
          <ul class="activity-list">
            <li class="activity-empty">Aucune activité récente</li>
          </ul>
        }
      >
        <ul class="activity-list" ref={autoAnimateList}>
          <For each={visibleRows()}>{(row) => <Row row={row} />}</For>
          <Show when={hasMore()}>
            <li class="activity-show-more">
              <button
                type="button"
                class="btn-link"
                onClick={() => setActivityExpanded(!activity.expanded)}
              >
                {activity.expanded ? 'Voir moins' : `Tout voir (${activityRows().length})`}
              </button>
            </li>
          </Show>
        </ul>
      </Show>
    </div>
  );
};
