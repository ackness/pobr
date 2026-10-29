import { describe, expect, it } from 'vitest';
import { looksLikeBuild } from './pasteImport';
import { SHORTCUT_ROUTES, shortcutAction } from './shortcuts';

const key = (key: string, extra: Partial<Parameters<typeof shortcutAction>[0]> = {}) => ({ key, metaKey: false, ctrlKey: false, altKey: false, ...extra });

describe('shortcutAction', () => {
  it('opens the palette with ⌘K or Ctrl+K, even while typing', () => {
    expect(shortcutAction(key('k', { metaKey: true }), true)).toEqual({ type: 'palette' });
    expect(shortcutAction(key('K', { ctrlKey: true }), false)).toEqual({ type: 'palette' });
  });
  it('maps number keys to the top bar from left to right', () => {
    expect(SHORTCUT_ROUTES[0]).toBe('overview');
    expect(shortcutAction(key('1'), false)).toEqual({ type: 'route', route: 'overview' });
    expect(shortcutAction(key(String(SHORTCUT_ROUTES.length)), false)).toEqual({ type: 'route', route: SHORTCUT_ROUTES.at(-1) });
    expect(shortcutAction(key(String(SHORTCUT_ROUTES.length + 1)), false)).toBeNull();
    expect(shortcutAction(key('0'), false)).toBeNull();
  });
  it('never steals keys while the user is typing or holding a modifier', () => {
    for (const k of ['1', '?', '[', 'Escape']) expect(shortcutAction(key(k), true)).toBeNull();
    expect(shortcutAction(key('1', { ctrlKey: true }), false)).toBeNull();
    expect(shortcutAction(key('[', { altKey: true }), false)).toBeNull();
  });
  it('knows the help, rail and close keys', () => {
    expect(shortcutAction(key('?'), false)).toEqual({ type: 'help' });
    expect(shortcutAction(key('['), false)).toEqual({ type: 'rail' });
    expect(shortcutAction(key('Escape'), false)).toEqual({ type: 'close' });
  });
});

describe('undo and redo keys', () => {
  it('map ⌘Z / Ctrl+Z to undo, with Shift or Ctrl+Y to redo, outside text fields', () => {
    expect(shortcutAction(key('z', { ctrlKey: true }), false)).toEqual({ type: 'undo' });
    expect(shortcutAction(key('z', { metaKey: true, shiftKey: true }), false)).toEqual({ type: 'redo' });
    expect(shortcutAction(key('y', { ctrlKey: true }), false)).toEqual({ type: 'redo' });
  });
  it('leave them to the browser while typing', () => {
    expect(shortcutAction(key('z', { ctrlKey: true }), true)).toBeNull();
    expect(shortcutAction(key('y', { ctrlKey: true }), true)).toBeNull();
  });
});

describe('looksLikeBuild', () => {
  it('accepts a long PoB code and links to build hosts', () => {
    expect(looksLikeBuild('eNrt'.repeat(30))).toBe(true);
    expect(looksLikeBuild('https://pobb.in/abc123')).toBe(true);
    expect(looksLikeBuild('https://poe.ninja/poe2/pob/xyz')).toBe(true);
  });
  it('ignores ordinary text, short strings and other links', () => {
    expect(looksLikeBuild('hello world')).toBe(false);
    expect(looksLikeBuild('abc123')).toBe(false);
    expect(looksLikeBuild('https://example.com/page')).toBe(false);
    expect(looksLikeBuild('')).toBe(false);
    expect(looksLikeBuild('Rarity: RARE\nItem Name\n' + 'x'.repeat(100))).toBe(false);
  });
});
