import { For, type JSX } from 'solid-js';

export interface PinDurationOption {
  label: string;
  secs: number;
}

export interface PinDurationPickerProps {
  value: number;
  onChange: (secs: number) => void;
  options: PinDurationOption[];
  ariaLabel?: string;
}

/** Pill group for picking a pin TTL.  Pure UI — the parent is responsible for
 *  turning the chosen duration into an IPC call. */
export const PinDurationPicker = (props: PinDurationPickerProps): JSX.Element => {
  return (
    <div
      class="pin-duration"
      role="radiogroup"
      aria-label={props.ariaLabel ?? 'Durée de l’épinglage'}
    >
      <For each={props.options}>
        {(opt) => {
          const selected = () => props.value === opt.secs;
          return (
            <button
              type="button"
              class="pin-duration-option"
              classList={{ active: selected() }}
              role="radio"
              aria-checked={selected()}
              onClick={() => props.onChange(opt.secs)}
            >
              {opt.label}
            </button>
          );
        }}
      </For>
    </div>
  );
};
