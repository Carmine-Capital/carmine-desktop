import { For, Show, createMemo, createSignal, onCleanup, type JSX } from 'solid-js';

import { dismissToast, toasts, type Toast, type ToastKind } from '../store/toasts';
import { CloseIcon } from './Icons';

const MAX_VISIBLE = 3;
const DURATIONS: Record<ToastKind, number> = {
  success: 5000,
  info: 5000,
  warning: 5000,
  error: 10000,
};

const KIND_ICON: Record<ToastKind, string> = {
  success: '✓',
  error: '✕',
  info: 'i',
  warning: '!',
};

/** One toast row.  Owns its own auto-dismiss timer which pauses while hovered
 *  and resets on pointer-leave so users always get the full remaining window
 *  after their last interaction. */
const ToastItem = (props: { toast: Toast }): JSX.Element => {
  const [paused, setPaused] = createSignal(false);
  const [actionBusy, setActionBusy] = createSignal(false);

  let timer: number | null = null;
  let remaining = DURATIONS[props.toast.kind];
  let startedAt = Date.now();

  const scheduleDismiss = (ms: number) => {
    if (timer !== null) {
      window.clearTimeout(timer);
      timer = null;
    }
    startedAt = Date.now();
    timer = window.setTimeout(() => {
      timer = null;
      dismissToast(props.toast.id);
    }, ms);
  };

  scheduleDismiss(remaining);

  onCleanup(() => {
    if (timer !== null) {
      window.clearTimeout(timer);
      timer = null;
    }
  });

  const onEnter = () => {
    if (paused()) return;
    setPaused(true);
    if (timer !== null) {
      remaining = Math.max(0, remaining - (Date.now() - startedAt));
      window.clearTimeout(timer);
      timer = null;
    }
  };

  const onLeave = () => {
    if (!paused()) return;
    setPaused(false);
    scheduleDismiss(remaining);
  };

  const onAction = async () => {
    const action = props.toast.action;
    if (!action || actionBusy()) return;
    setActionBusy(true);
    try {
      await action.onClick();
    } finally {
      setActionBusy(false);
      dismissToast(props.toast.id);
    }
  };

  const onClose = () => {
    dismissToast(props.toast.id);
  };

  return (
    <div
      class={`toast toast-${props.toast.kind}`}
      role={props.toast.kind === 'error' ? 'alert' : 'status'}
      aria-live={props.toast.kind === 'error' ? 'assertive' : 'polite'}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onFocusIn={onEnter}
      onFocusOut={onLeave}
    >
      <span class={`toast-icon toast-icon-${props.toast.kind}`} aria-hidden="true">
        {KIND_ICON[props.toast.kind]}
      </span>
      <div class="toast-body">
        <div class="toast-title">{props.toast.title}</div>
        <Show when={props.toast.message}>
          <div class="toast-message">{props.toast.message}</div>
        </Show>
        <Show when={props.toast.action}>
          <button
            type="button"
            class="toast-action btn-link"
            onClick={onAction}
            disabled={actionBusy()}
          >
            {props.toast.action!.label}
          </button>
        </Show>
      </div>
      <button
        type="button"
        class="toast-close"
        aria-label="Fermer"
        onClick={onClose}
      >
        <CloseIcon size={12} />
      </button>
    </div>
  );
};

/** Fixed bottom-right stack.  Keeps only the newest `MAX_VISIBLE` toasts
 *  mounted — older ones are removed from the store after being shown past the
 *  cap, which triggers their exit animation via `@keyframes toast-out`. */
export const ToastHost = (): JSX.Element => {
  const visible = createMemo(() => {
    const list = toasts.list;
    if (list.length <= MAX_VISIBLE) return list;
    return list.slice(list.length - MAX_VISIBLE);
  });

  return (
    <div class="toast-host" aria-live="polite" aria-atomic="false">
      <For each={visible()}>{(toast) => <ToastItem toast={toast} />}</For>
    </div>
  );
};
