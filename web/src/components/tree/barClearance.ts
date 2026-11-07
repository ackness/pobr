/** CSS variable on the bar's parent: how far down the bar reaches, for overlays that sit under it. */
export const BAR_BOTTOM_VAR = '--tree-top-bar-bottom';

/**
 * Publish where `bar` ends (its offset in the parent plus its height) and keep it current. The bar wraps to
 * more rows as the canvas narrows, so a fixed `top` under it ends up beneath the bar. Shaped as a React ref
 * callback: the returned function is the cleanup.
 */
export function trackBarBottom(bar: HTMLElement | null): (() => void) | undefined {
  const parent = bar?.parentElement;
  if (!bar || !parent || typeof ResizeObserver === 'undefined') return undefined;
  const observer = new ResizeObserver(() => parent.style.setProperty(BAR_BOTTOM_VAR, `${bar.offsetTop + bar.offsetHeight}px`));
  observer.observe(bar);
  return () => {
    observer.disconnect();
    parent.style.removeProperty(BAR_BOTTOM_VAR);
  };
}
