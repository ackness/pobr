import { useLayoutEffect, type RefObject } from 'react';

/** Keep inline menus inside the scroll viewport, opening upward when needed. */
export function usePopupPosition(
  rootRef: RefObject<HTMLElement | null>,
  panelRef: RefObject<HTMLElement | null>,
  open: boolean,
  contentKey: unknown,
  maxHeight = 280,
) {
  useLayoutEffect(() => {
    if (!open) return;
    const position = () => {
      const root = rootRef.current;
      const panel = panelRef.current;
      if (!root || !panel) return;
      const anchor = root.getBoundingClientRect();
      const bounds = root.closest('main')?.getBoundingClientRect();
      const top = Math.max(0, bounds?.top ?? 0) + 8;
      const bottom = Math.min(window.innerHeight, bounds?.bottom ?? window.innerHeight) - 8;
      const below = Math.max(0, bottom - anchor.bottom - 4);
      const above = Math.max(0, anchor.top - top - 4);
      panel.style.maxHeight = `${maxHeight}px`;
      const upward = panel.getBoundingClientRect().height > below && above > below;
      panel.style.top = upward ? 'auto' : 'calc(100% + 4px)';
      panel.style.bottom = upward ? 'calc(100% + 4px)' : 'auto';
      panel.style.maxHeight = `${Math.min(maxHeight, upward ? above : below)}px`;
      panel.style.left = '0px';
      const rect = panel.getBoundingClientRect();
      const left = Math.max(0, bounds?.left ?? 0) + 16;
      const right = Math.min(document.documentElement.clientWidth, bounds?.right ?? document.documentElement.clientWidth) - 16;
      panel.style.left = `${Math.max(left - rect.left, Math.min(0, right - rect.right))}px`;
    };
    position();
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
    };
  }, [rootRef, panelRef, open, contentKey, maxHeight]);
}
