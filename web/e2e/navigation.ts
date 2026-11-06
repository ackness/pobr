import type { Page } from '@playwright/test';

/** The URL changes inside the transition, before its overlay releases the page. */
export async function waitForPageTransition(page: Page): Promise<void> {
  await page.evaluate(async () => {
    // Allow the new page snapshot and its transition animations to be created.
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    const transitions = document.getAnimations().filter(animation => {
      const effect = animation.effect as KeyframeEffect | null;
      const name = (animation as CSSAnimation).animationName;
      return effect?.pseudoElement?.startsWith('::view-transition') || name === 'shell-enter' || name === 'shell-leave';
    });
    await Promise.all(transitions.map(animation => animation.finished.catch(() => {})));
  });
}
