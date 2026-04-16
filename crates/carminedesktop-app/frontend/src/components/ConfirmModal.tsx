import { Show, createEffect, onCleanup, type JSX } from 'solid-js';

import { confirmRequest, resolveConfirm } from '../store/confirm';

const FOCUSABLE_SELECTOR =
  'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/** Global confirm dialog driven by `confirmRequest`.  Only one instance should
 *  be mounted — the App shell owns it.  Implements a minimal focus trap: on
 *  open the confirm button receives focus, Tab cycles within the dialog, and
 *  Esc/overlay click resolve the request as cancel. */
export const ConfirmModal = (): JSX.Element => {
  let dialogRef: HTMLDivElement | undefined;
  let confirmBtnRef: HTMLButtonElement | undefined;
  let lastFocus: HTMLElement | null = null;

  const cancel = () => resolveConfirm(false);
  const confirmChoice = () => resolveConfirm(true);

  const onKeyDown = (e: KeyboardEvent) => {
    if (!confirmRequest()) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      cancel();
      return;
    }
    if (e.key === 'Enter') {
      const target = e.target as HTMLElement | null;
      // Don't swallow Enter when it's already firing a button, that would
      // double-resolve.  Default browser behaviour already routes Enter to
      // the focused button, so skip the shortcut in that case.
      if (target && target.tagName === 'BUTTON') return;
      e.preventDefault();
      confirmChoice();
      return;
    }
    if (e.key === 'Tab') {
      if (!dialogRef) return;
      const focusable = Array.from(
        dialogRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      ).filter((el) => !el.hasAttribute('disabled'));
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  // Drive lifecycle from the reactive request so mount/unmount stays in sync
  // with the imperative API.
  createEffect(() => {
    const req = confirmRequest();
    if (req) {
      lastFocus = document.activeElement as HTMLElement | null;
      document.addEventListener('keydown', onKeyDown, true);
      // Defer focus to after the <Show> mounts the dialog.
      queueMicrotask(() => {
        confirmBtnRef?.focus();
      });
    } else {
      document.removeEventListener('keydown', onKeyDown, true);
      if (lastFocus && typeof lastFocus.focus === 'function') {
        lastFocus.focus();
      }
      lastFocus = null;
    }
  });

  onCleanup(() => {
    document.removeEventListener('keydown', onKeyDown, true);
  });

  const onOverlayClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) cancel();
  };

  return (
    <Show when={confirmRequest()}>
      {(req) => {
        const opts = () => req().options;
        return (
          <div
            class="modal-overlay"
            role="presentation"
            onMouseDown={onOverlayClick}
          >
            <div
              ref={dialogRef}
              class="modal-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby={`confirm-title-${req().id}`}
              aria-describedby={`confirm-message-${req().id}`}
            >
              <h3 id={`confirm-title-${req().id}`} class="modal-title">
                {opts().title}
              </h3>
              <p id={`confirm-message-${req().id}`} class="modal-message">
                {opts().message}
              </p>
              <div class="modal-actions">
                <button
                  type="button"
                  class="btn-ghost"
                  onClick={cancel}
                >
                  {opts().cancelLabel ?? 'Annuler'}
                </button>
                <button
                  ref={confirmBtnRef}
                  type="button"
                  class={opts().danger ? 'btn-danger-solid' : 'btn-primary'}
                  onClick={confirmChoice}
                >
                  {opts().confirmLabel ?? 'Confirmer'}
                </button>
              </div>
            </div>
          </div>
        );
      }}
    </Show>
  );
};
