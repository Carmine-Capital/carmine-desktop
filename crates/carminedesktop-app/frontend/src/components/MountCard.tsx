import { Match, Switch, createSignal, type JSX } from 'solid-js';

import { OneDriveIcon, SharePointIcon } from './Icons';
import { showStatus, formatError } from './StatusBar';

export type MountKind = 'onedrive' | 'sharepoint';

export interface MountCardProps {
  kind: MountKind;
  name: string;
  mountPoint: string | null;
  isMounted: boolean;
  /** Called when the toggle is flipped.  The caller owns the IPC round-trip. */
  onToggle: (next: boolean) => Promise<void>;
  /** Whether toggling is currently disabled (e.g. missing site info for SP). */
  disabled?: boolean;
}

/** A single library row — OneDrive or SharePoint.  Fine-grained reactivity
 *  means switching from "mounted" to "unmounted" only rewrites the data-mounted
 *  attribute and the affected pill label; siblings and the icon background
 *  animate via the CSS `data-mounted` selector without DOM churn. */
export const MountCard = (props: MountCardProps): JSX.Element => {
  const [busy, setBusy] = createSignal(false);

  const toggle = async (e: Event) => {
    const next = (e.currentTarget as HTMLInputElement).checked;
    if (props.disabled) {
      (e.currentTarget as HTMLInputElement).checked = !next;
      return;
    }
    setBusy(true);
    try {
      await props.onToggle(next);
    } catch (err) {
      (e.currentTarget as HTMLInputElement).checked = !next;
      // Caller signals "user cancelled the confirm" with a sentinel — revert
      // the toggle state but skip the error banner since nothing went wrong.
      const msg = err instanceof Error ? err.message : String(err);
      if (msg !== '__cancelled__') {
        showStatus(formatError(err), 'error');
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <li class="mount-card" data-mounted={props.isMounted ? 'true' : 'false'} data-kind={props.kind}>
      <div class="mount-icon" aria-hidden="true">
        <Switch>
          <Match when={props.kind === 'onedrive'}>
            <OneDriveIcon />
          </Match>
          <Match when={props.kind === 'sharepoint'}>
            <SharePointIcon />
          </Match>
        </Switch>
      </div>
      <div class="mount-body">
        <div class="mount-head">
          <div class="mount-title">{props.name}</div>
          <div class="mount-path" title={props.isMounted ? props.mountPoint ?? undefined : undefined}>
            {props.isMounted ? props.mountPoint : 'Non monté'}
          </div>
        </div>
        <div class="mount-pills">
          <span class={`mount-pill mount-pill-status ${props.isMounted ? 'mounted' : 'unmounted'}`}>
            <span class="mount-pill-dot" />
            {props.isMounted ? 'Monté' : 'Non monté'}
          </span>
        </div>
      </div>
      <label class="toggle-switch mount-toggle">
        <input
          type="checkbox"
          checked={props.isMounted}
          disabled={busy() || props.disabled}
          aria-busy={busy()}
          onChange={toggle}
        />
        <span class="toggle-track" />
        <span class="visually-hidden">Activer {props.name}</span>
      </label>
    </li>
  );
};
