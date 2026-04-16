import { For, Show, type JSX } from 'solid-js';

export interface QuickStat {
  label: string;
  value: string;
  hint?: string;
}

/** Horizontal grid of small stat cards for dashboards and headers.  Pure
 *  presentation — the parent owns the data and reactivity. */
export const QuickStats = (props: { items: QuickStat[] }): JSX.Element => {
  return (
    <div class="quick-stats">
      <For each={props.items}>
        {(item) => (
          <div class="quick-stat-card">
            <div class="quick-stat-label">{item.label}</div>
            <div class="quick-stat-value">{item.value}</div>
            <Show when={item.hint}>
              <div class="quick-stat-hint">{item.hint}</div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
};
