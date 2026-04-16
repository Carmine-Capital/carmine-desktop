import { For, Show, createMemo, type JSX } from 'solid-js';

import type { ActivityEntry } from '../bindings';
import { activity, setActivityExpanded } from '../store/activity';
import { formatRelativeTime, truncatePath } from '../utils/format';
import { autoAnimateList } from './autoAnimate';

const ACTIVITY_TYPE_LABEL: Record<string, string> = {
  synced: 'synchronisé',
  uploaded: 'envoyé',
  deleted: 'supprimé',
  conflict: 'conflit',
};

const COLLAPSED_LIMIT = 10;

const ActivityRow = (props: { entry: ActivityEntry }): JSX.Element => {
  const tagClass = createMemo(() => `activity-tag ${props.entry.activityType}`);
  const tagLabel = createMemo(
    () => ACTIVITY_TYPE_LABEL[props.entry.activityType] ?? props.entry.activityType,
  );
  const name = createMemo(() => truncatePath(props.entry.filePath));
  const time = createMemo(() => formatRelativeTime(props.entry.timestamp));
  return (
    <li class="activity-row">
      <span class={tagClass()}>{tagLabel()}</span>
      <span class="activity-name">{name()}</span>
      <span class="activity-time">{time()}</span>
    </li>
  );
};

export const ActivityFeed = (): JSX.Element => {
  const visible = createMemo(() => {
    const list = activity.entries;
    if (activity.expanded) return list;
    return list.slice(0, COLLAPSED_LIMIT);
  });
  const hasMore = () => activity.entries.length > COLLAPSED_LIMIT;

  return (
    <Show
      when={activity.entries.length > 0}
      fallback={
        <ul class="activity-list">
          <li class="activity-empty">Aucune activité récente</li>
        </ul>
      }
    >
      <ul class="activity-list" ref={autoAnimateList}>
        <For each={visible()}>{(entry) => <ActivityRow entry={entry} />}</For>
        <Show when={hasMore()}>
          <li class="activity-show-more">
            <button
              type="button"
              class="btn-link"
              onClick={() => setActivityExpanded(!activity.expanded)}
            >
              {activity.expanded
                ? 'Voir moins'
                : `Tout voir (${activity.entries.length})`}
            </button>
          </li>
        </Show>
      </ul>
    </Show>
  );
};
