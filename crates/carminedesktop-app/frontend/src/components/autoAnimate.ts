// Thin wrapper so a component can just write `ref={autoAnimateList}` without
// pulling the @formkit package into every panel that wants list animations.

import autoAnimate from '@formkit/auto-animate';

export function autoAnimateList(el: HTMLElement): void {
  autoAnimate(el, { duration: 180, easing: 'ease-out' });
}
