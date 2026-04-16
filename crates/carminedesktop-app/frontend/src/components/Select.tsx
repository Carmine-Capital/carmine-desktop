import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  onCleanup,
  type JSX,
} from 'solid-js';

export interface SelectOption<T extends string | number> {
  value: T;
  label: string;
}

export interface SelectProps<T extends string | number> {
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  ariaLabel: string;
  placeholder?: string;
  class?: string;
  disabled?: boolean;
}

/** Custom dropdown that mirrors a native <select> but renders its option panel
 *  with the glass theme.  Chromium's WebView2 paints native <option> lists with
 *  the OS default (white on Windows), which clashes with the dark app surface.
 *
 *  Keyboard-wise the trigger stays focused when the panel is open; a global
 *  keydown listener (capture phase) handles ArrowUp/Down/Enter/Esc.  This
 *  mirrors the approach in ConfirmModal and avoids juggling focus between the
 *  button and the list. */
export function Select<T extends string | number>(
  props: SelectProps<T>,
): JSX.Element {
  let triggerRef: HTMLButtonElement | undefined;
  let panelRef: HTMLUListElement | undefined;

  const [open, setOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(-1);
  const listId = `select-${createUniqueId()}`;

  const selectedIndex = createMemo(() =>
    props.options.findIndex((o) => o.value === props.value),
  );
  const selectedLabel = createMemo(() => {
    const idx = selectedIndex();
    return idx >= 0 ? props.options[idx]!.label : (props.placeholder ?? '');
  });

  const close = (returnFocus = true) => {
    setOpen(false);
    setActiveIndex(-1);
    if (returnFocus) triggerRef?.focus();
  };

  const openPanel = () => {
    if (props.disabled) return;
    setActiveIndex(selectedIndex() >= 0 ? selectedIndex() : 0);
    setOpen(true);
  };

  const toggle = () => (open() ? close() : openPanel());

  const commit = (idx: number) => {
    const opt = props.options[idx];
    if (!opt) return;
    props.onChange(opt.value);
    close();
  };

  const onTriggerKey = (e: KeyboardEvent) => {
    if (props.disabled) return;
    // If the panel is open the document-level handler owns these keys; don't
    // double-handle them here.
    if (open()) return;
    switch (e.key) {
      case 'ArrowDown':
      case 'ArrowUp':
      case 'Enter':
      case ' ':
        e.preventDefault();
        openPanel();
        break;
    }
  };

  const onDocumentKey = (e: KeyboardEvent) => {
    if (!open()) return;
    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        const n = props.options.length;
        if (n > 0) setActiveIndex((i) => (i < 0 ? 0 : (i + 1) % n));
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        const n = props.options.length;
        if (n > 0) setActiveIndex((i) => (i < 0 ? n - 1 : (i - 1 + n) % n));
        break;
      }
      case 'Home':
        e.preventDefault();
        setActiveIndex(0);
        break;
      case 'End':
        e.preventDefault();
        setActiveIndex(props.options.length - 1);
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        if (activeIndex() >= 0) commit(activeIndex());
        break;
      case 'Escape':
        e.preventDefault();
        close();
        break;
      case 'Tab':
        // Let the Tab keypress close the panel but still move focus naturally.
        close(false);
        break;
    }
  };

  const onDocumentPointer = (e: PointerEvent) => {
    if (!open()) return;
    const target = e.target as Node | null;
    if (!target) return;
    if (triggerRef?.contains(target)) return;
    if (panelRef?.contains(target)) return;
    close(false);
  };

  createEffect(() => {
    if (open()) {
      document.addEventListener('keydown', onDocumentKey, true);
      document.addEventListener('pointerdown', onDocumentPointer, true);
      // Scroll the active option into view once the panel mounts.
      queueMicrotask(() => {
        const idx = activeIndex();
        if (idx < 0 || !panelRef) return;
        const item = panelRef.querySelectorAll<HTMLLIElement>('[role="option"]')[idx];
        item?.scrollIntoView({ block: 'nearest' });
      });
    } else {
      document.removeEventListener('keydown', onDocumentKey, true);
      document.removeEventListener('pointerdown', onDocumentPointer, true);
    }
  });

  onCleanup(() => {
    document.removeEventListener('keydown', onDocumentKey, true);
    document.removeEventListener('pointerdown', onDocumentPointer, true);
  });

  return (
    <div class={`custom-select ${props.class ?? ''}`.trim()} classList={{ open: open() }}>
      <button
        ref={triggerRef}
        type="button"
        class="custom-select-trigger"
        aria-label={props.ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-controls={listId}
        disabled={props.disabled}
        onClick={toggle}
        onKeyDown={onTriggerKey}
      >
        <span class="custom-select-value">{selectedLabel()}</span>
        <span class="custom-select-chevron" aria-hidden="true">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </span>
      </button>
      <Show when={open()}>
        <ul
          ref={panelRef}
          id={listId}
          class="custom-select-panel"
          role="listbox"
          aria-label={props.ariaLabel}
        >
          <For each={props.options}>
            {(opt, idx) => {
              const isSelected = () => props.value === opt.value;
              const isActive = () => activeIndex() === idx();
              return (
                <li
                  class="custom-select-option"
                  classList={{ selected: isSelected(), active: isActive() }}
                  role="option"
                  aria-selected={isSelected()}
                  onPointerEnter={() => setActiveIndex(idx())}
                  onClick={() => commit(idx())}
                >
                  <span class="custom-select-option-label">{opt.label}</span>
                  <Show when={isSelected()}>
                    <span class="custom-select-check" aria-hidden="true">
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <polyline points="20 6 9 17 4 12" />
                      </svg>
                    </span>
                  </Show>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </div>
  );
}
