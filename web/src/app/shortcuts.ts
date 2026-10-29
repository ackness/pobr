import { NAV_GROUPS, type Route } from './routes';

/** Pages in the order of the number keys: the top bar from left to right. */
export const SHORTCUT_ROUTES: readonly Route[] = NAV_GROUPS.flat();

export type ShortcutAction =
  | { type: 'palette' } | { type: 'help' } | { type: 'rail' } | { type: 'close' } | { type: 'undo' } | { type: 'redo' } | { type: 'route'; route: Route };

interface KeyEventLike { key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey?: boolean }

/** What a key press means, given whether the user is typing. Only ⌘K / Ctrl+K works while typing. */
export function shortcutAction(event: KeyEventLike, typing: boolean): ShortcutAction | null {
  if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === 'k') return { type: 'palette' };
  // Undo and redo are the page's own only outside text fields; inside one the browser's undo applies.
  if (!typing && (event.metaKey || event.ctrlKey) && !event.altKey) {
    const key = event.key.toLowerCase();
    if (key === 'z') return { type: event.shiftKey ? 'redo' : 'undo' };
    if (key === 'y') return { type: 'redo' };
  }
  if (typing || event.metaKey || event.ctrlKey || event.altKey) return null;
  if (event.key === '?') return { type: 'help' };
  if (event.key === '[') return { type: 'rail' };
  if (event.key === 'Escape') return { type: 'close' };
  const index = /^[1-9]$/.test(event.key) ? Number(event.key) - 1 : -1;
  return index >= 0 && index < SHORTCUT_ROUTES.length ? { type: 'route', route: SHORTCUT_ROUTES[index] } : null;
}

export function isTyping(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"], [role="combobox"], [role="textbox"], [role="searchbox"]');
}
