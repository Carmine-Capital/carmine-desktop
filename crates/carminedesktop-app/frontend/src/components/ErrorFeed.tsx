import { For, Show, createMemo, createSignal, type JSX } from 'solid-js';

import type { DashboardError } from '../bindings';
import { invoke } from '../ipc';
import { dismissError, errors } from '../store/errors';
import { mountForDrive } from '../store/mounts';
import { pushToast } from '../store/toasts';
import { formatError } from './StatusBar';
import { formatRelativeTime } from '../utils/format';
import { autoAnimateList } from './autoAnimate';

const ERROR_TYPE_LABEL: Record<string, string> = {
  conflict: 'conflit',
  writeback_failed: 'échec écriture',
  upload_failed: 'échec envoi',
};

/** Stable identity for an error entry.  `DashboardError` has no backend id so
 *  we derive one from the tuple that uniquely identifies it in the ring. */
const errorKey = (e: DashboardError): string =>
  `${e.driveId ?? '-'}|${e.fileName ?? '-'}|${e.errorType}|${e.timestamp}`;

const [expandedKey, setExpandedKey] = createSignal<string | null>(null);

const ErrorEntry = (props: { error: DashboardError }): JSX.Element => {
  const entryClass = createMemo(
    () => `error-entry${props.error.errorType === 'conflict' ? ' conflict' : ''}`,
  );
  const file = createMemo(() => props.error.fileName ?? 'Fichier inconnu');
  const type = createMemo(() => {
    const label = ERROR_TYPE_LABEL[props.error.errorType] ?? props.error.errorType ?? 'erreur';
    return `– ${label}`;
  });
  const time = createMemo(() => formatRelativeTime(props.error.timestamp));
  const key = createMemo(() => errorKey(props.error));
  const isExpanded = createMemo(() => expandedKey() === key());
  const canRetry = createMemo(() => {
    const did = props.error.driveId;
    return !!did && !!mountForDrive(did);
  });

  const [retrying, setRetrying] = createSignal(false);

  const handleRetry = async () => {
    const driveId = props.error.driveId;
    if (!driveId) return;
    const mount = mountForDrive(driveId);
    if (!mount) {
      pushToast({
        kind: 'error',
        title: 'Impossible de réessayer',
        message: 'Aucun montage actif n\u2019est associé à ce lecteur.',
      });
      return;
    }
    setRetrying(true);
    pushToast({ kind: 'info', title: 'Synchronisation en cours\u2026' });
    try {
      await invoke<void>('refresh_mount', { id: mount.id });
      pushToast({ kind: 'success', title: 'Synchronisation lancée' });
    } catch (e) {
      pushToast({
        kind: 'error',
        title: 'Échec de la synchronisation',
        message: formatError(e),
      });
    } finally {
      setRetrying(false);
    }
  };

  const handleDismiss = () => {
    if (isExpanded()) setExpandedKey(null);
    dismissError(props.error);
  };

  const toggleDetails = () => {
    setExpandedKey(isExpanded() ? null : key());
  };

  const formattedTimestamp = createMemo(() => {
    try {
      const d = new Date(props.error.timestamp);
      if (Number.isNaN(d.getTime())) return props.error.timestamp;
      return d.toLocaleString('fr-FR');
    } catch {
      return props.error.timestamp;
    }
  });

  return (
    <div class={entryClass()}>
      <div class="error-header">
        <span class="error-file">{file()}</span>
        <span class="error-type">{type()}</span>
        <span class="error-time">{time()}</span>
      </div>
      <Show when={props.error.message}>
        <div class="error-message">{props.error.message}</div>
      </Show>
      <div class="error-actions">
        <button
          type="button"
          class="btn-ghost btn-sm error-action"
          onClick={handleRetry}
          disabled={!canRetry() || retrying()}
          title={
            canRetry()
              ? 'Relancer la synchronisation du lecteur associé'
              : 'Aucun lecteur associé à cette erreur'
          }
        >
          {retrying() ? 'Synchronisation\u2026' : 'Réessayer'}
        </button>
        <button
          type="button"
          class="btn-ghost btn-sm error-action"
          onClick={handleDismiss}
          title="Retirer cette erreur de la liste"
        >
          Ignorer
        </button>
        <button
          type="button"
          class="btn-ghost btn-sm error-action"
          onClick={toggleDetails}
          aria-expanded={isExpanded()}
          title="Afficher le détail de l’erreur"
        >
          {isExpanded() ? 'Masquer' : 'Détails'}
        </button>
      </div>
      <Show when={isExpanded()}>
        <div class="error-details">
          <dl class="error-details-grid">
            <dt>Fichier</dt>
            <dd>{props.error.fileName ?? '—'}</dd>
            <dt>Chemin distant</dt>
            <dd class="error-details-path">{props.error.remotePath ?? '—'}</dd>
            <dt>Type</dt>
            <dd>{props.error.errorType}</dd>
            <dt>Lecteur</dt>
            <dd>{props.error.driveId ?? '—'}</dd>
            <dt>Horodatage</dt>
            <dd>{formattedTimestamp()}</dd>
            <Show when={props.error.actionHint}>
              <dt>Action suggérée</dt>
              <dd>{props.error.actionHint}</dd>
            </Show>
          </dl>
          <Show when={props.error.message}>
            <div class="error-details-message">{props.error.message}</div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export const ErrorFeed = (): JSX.Element => (
  <Show
    when={errors.entries.length > 0}
    fallback={<div class="error-empty">Aucune erreur</div>}
  >
    <div class="error-list" ref={autoAnimateList}>
      <For each={errors.entries}>
        {(error) => <ErrorEntry error={error} />}
      </For>
    </div>
  </Show>
);
