import { Dynamic } from 'solid-js/web';
import type { JSX } from 'solid-js';

type SkeletonVariant = 'pin' | 'card' | 'row';

/**  One shared shape for all loading placeholders so swapping skeleton → real
 *   content never triggers a layout shift.  Renders as `<li>` when used inside
 *   a list via `tag="li"`. */
export const Skeleton = (props: {
  variant?: SkeletonVariant;
  label?: string;
  tag?: 'div' | 'li';
}): JSX.Element => {
  const variant = () => props.variant ?? 'row';
  const tag = () => props.tag ?? 'div';
  return (
    <Dynamic
      component={tag()}
      class={`skeleton skeleton-${variant()}`}
      aria-busy="true"
      aria-live="polite"
    >
      <span class="spinner" />
      {props.label ? <span class="skeleton-title">{props.label}</span> : null}
    </Dynamic>
  );
};
