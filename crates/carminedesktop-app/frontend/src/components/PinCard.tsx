import { Show, createMemo, createSignal, type JSX } from 'solid-js';

import { api } from '../ipc';
import type { PinView } from '../store/pins';
import { confirm } from '../store/confirm';
import { pushToast } from '../store/toasts';
import { showStatus, formatError } from './StatusBar';
import { CloseIcon, FolderIcon } from './Icons';
import { PinDurationPicker } from './PinDurationPicker';
import { DEFAULT_PIN_DURATION_OPTIONS } from './pinDurationOptions';

const PIN_STATUS_LABEL: Record<string, string> = {
  downloaded: 'disponible',
  partial: 'partiel',
  stale: 'obsolète',
  analyzing: 'analyse…',
  expired: 'expiré',
  unknown: '—',
};

interface ExpiryInfo {
  text: string;
  expired: boolean;
}

function formatTimeRemaining(expiresAt: string): ExpiryInfo {
  const expires = new Date(`${expiresAt}Z`).getTime();
  const diff = expires - Date.now();
  if (diff <= 0) return { text: 'Expiré', expired: true };
  const hours = Math.floor(diff / 3_600_000);
  const days = Math.floor(hours / 24);
  if (days > 0) return { text: `Reste ${days}j ${hours % 24}h`, expired: false };
  const mins = Math.floor((diff % 3_600_000) / 60_000);
  if (hours > 0) return { text: `Reste ${hours}h ${mins}m`, expired: false };
  return { text: `Reste ${mins}m`, expired: false };
}

function healthKey(pin: PinView, expired: boolean): string {
  if (expired) return 'expired';
  if (pin.totalFiles === 0) return 'analyzing';
  return pin.status;
}

/** A single pin row.  Reads individual fields from its store proxy so that a
 *  `pin:health` update (e.g. `cachedFiles` 13→14) re-runs only the bound
 *  effects — the health/expiry pills and sibling rows stay untouched. */
export const PinCard = (props: { pin: PinView }): JSX.Element => {
  const expiry = createMemo(() => formatTimeRemaining(props.pin.expiresAt));
  const key = createMemo(() => healthKey(props.pin, expiry().expired));
  const title = createMemo(() =>
    props.pin.folderName === 'root' ? props.pin.mountName : props.pin.folderName,
  );
  const percent = createMemo(() => {
    if (props.pin.totalFiles <= 0) return 0;
    return Math.min(100, (props.pin.cachedFiles / props.pin.totalFiles) * 100);
  });

  // Local expand/collapse for the per-pin TTL picker.  Keeping the UI inline
  // (rather than a floating popover) avoids another positioning system and
  // still fits the list-item layout.
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [extending, setExtending] = createSignal(false);

  const extend = async (ttlSecs: number) => {
    if (extending()) return;
    setExtending(true);
    try {
      await api.extendOfflinePin(props.pin.driveId, props.pin.itemId, ttlSecs);
      pushToast({ kind: 'success', title: 'Durée mise à jour' });
      setPickerOpen(false);
    } catch (e) {
      pushToast({ kind: 'error', title: 'Échec', message: formatError(e) });
    } finally {
      setExtending(false);
    }
  };

  const handleRemove = async () => {
    const name = props.pin.folderName;
    const ok = await confirm({
      title: 'Retirer du hors-ligne ?',
      message: 'Ce dossier ne sera plus disponible sans connexion.',
      confirmLabel: 'Retirer',
      danger: true,
    });
    if (!ok) return;
    try {
      await api.removeOfflinePin(props.pin.driveId, props.pin.itemId);
      pushToast({ kind: 'success', title: 'Dossier retiré' });
      showStatus(`Épinglage supprimé pour ${name}`, 'success');
    } catch (e) {
      pushToast({ kind: 'error', title: 'Échec du retrait', message: formatError(e) });
      showStatus(formatError(e), 'error');
    }
  };

  return (
    <li class="pin-card" data-health={key()}>
      <div class="pin-icon" aria-hidden="true">
        <FolderIcon />
      </div>
      <div class="pin-body">
        <div class="pin-head">
          <div class="pin-title">{title()}</div>
          <div class="pin-path" title={props.pin.mountName}>
            {props.pin.mountName}
          </div>
        </div>
        <div class="pin-pills">
          <span class={`pin-pill pin-pill-health ${key()}`}>
            <span class="pin-pill-dot" />
            <span class="pin-pill-label">{PIN_STATUS_LABEL[key()] ?? key()}</span>
          </span>
          <button
            type="button"
            class={`pin-pill pin-pill-expiry pin-pill-expiry-btn${expiry().expired ? ' expired' : ''}`}
            aria-expanded={pickerOpen()}
            aria-label="Modifier la durée"
            onClick={() => setPickerOpen((v) => !v)}
          >
            {expiry().text}
          </button>
          <Show when={props.pin.totalFiles > 0}>
            <span class="pin-pill pin-pill-count">
              {props.pin.cachedFiles} / {props.pin.totalFiles} fichiers
            </span>
          </Show>
        </div>
        <Show when={pickerOpen()}>
          <div class="pin-duration-inline" aria-busy={extending()}>
            <PinDurationPicker
              value={0}
              onChange={(secs) => void extend(secs)}
              options={DEFAULT_PIN_DURATION_OPTIONS}
              ariaLabel="Durée de l’épinglage"
            />
          </div>
        </Show>
        <Show when={props.pin.totalFiles > 0}>
          <div
            class="pin-progress"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={props.pin.totalFiles}
            aria-valuenow={props.pin.cachedFiles}
          >
            <div
              class={`pin-progress-fill ${key()}`}
              style={{ width: `${percent().toFixed(1)}%` }}
            />
          </div>
        </Show>
      </div>
      <button
        class="pin-remove"
        type="button"
        title="Supprimer l’accès hors-ligne"
        aria-label="Supprimer"
        onClick={handleRemove}
      >
        <CloseIcon />
      </button>
    </li>
  );
};
