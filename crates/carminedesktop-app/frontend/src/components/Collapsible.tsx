import { createSignal, createUniqueId, type JSX } from 'solid-js';

export interface CollapsibleProps {
  title: string;
  defaultOpen?: boolean;
  children: JSX.Element;
}

/** Disclosure widget with a chevron that rotates on open.  Uses the
 *  `grid-template-rows: 0fr → 1fr` trick to animate height without measuring
 *  the content — see styles.css `.collapsible-content`. */
export const Collapsible = (props: CollapsibleProps): JSX.Element => {
  const [open, setOpen] = createSignal(!!props.defaultOpen);
  const contentId = `collapsible-${createUniqueId()}`;

  const toggle = () => setOpen((v) => !v);

  return (
    <div class="collapsible" classList={{ open: open() }}>
      <button
        type="button"
        class="collapsible-header"
        aria-expanded={open()}
        aria-controls={contentId}
        onClick={toggle}
      >
        <span class="collapsible-title">{props.title}</span>
        <span class="collapsible-chevron" aria-hidden="true">
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
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </span>
      </button>
      <div id={contentId} class="collapsible-content" role="region">
        <div class="collapsible-inner">{props.children}</div>
      </div>
    </div>
  );
};
