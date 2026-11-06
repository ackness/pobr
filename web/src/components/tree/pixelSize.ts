/**
 * Size an element in pixels to its parent's content box and keep it in step; returns a stop function.
 *
 * The passive tree SVG used `width/height: 100%`. The browser resolves a percentage size again on every
 * layout of the block around it, and the SVG then lays out each of its ~10,000 shapes again: about 90 ms
 * in headless Chromium whenever anything beside the canvas changed (a line in the side column, a stat
 * after an allocation). With a size in pixels the shapes keep their layout. The CSS percentage stays as
 * the size until the first measurement and wherever ResizeObserver is missing.
 */
export function fitToParentInPixels(element: HTMLElement | SVGElement): () => void {
  const parent = element.parentElement;
  if (!parent || typeof ResizeObserver === 'undefined') return () => {};
  const observer = new ResizeObserver(entries => {
    const entry = entries[entries.length - 1];
    if (!entry) return;
    const box = entry.contentBoxSize?.[0];
    const width = box ? box.inlineSize : entry.contentRect.width;
    const height = box ? box.blockSize : entry.contentRect.height;
    element.style.width = `${width}px`;
    element.style.height = `${height}px`;
  });
  observer.observe(parent);
  return () => {
    observer.disconnect();
    element.style.removeProperty('width');
    element.style.removeProperty('height');
  };
}
