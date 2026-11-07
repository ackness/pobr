import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { afterEach, expect, test, vi } from 'vitest';
import { SIDE_DEFAULT, SIDE_MAX_SHARE, clampSideWidth, parseSideWidth, readSideWidth, sideWidthForKey, sideWidthLimits, writeSideWidth } from './sidebarWidth';

afterEach(() => vi.unstubAllGlobals());

test('limits follow the fixed range and give way to the 48% share of a small panel', () => {
  expect(sideWidthLimits()).toEqual({ min: 22, max: 44 });
  expect(sideWidthLimits(100)).toEqual({ min: 22, max: 44 });
  expect(sideWidthLimits(60)).toEqual({ min: 22, max: 28.8 });
  // Below 22 / 0.48 rem the share wins over the minimum, as the CSS min() does.
  expect(sideWidthLimits(40)).toEqual({ min: 19.2, max: 19.2 });
});

test('clamps to the range and rounds to a tenth', () => {
  expect(clampSideWidth(10)).toBe(22);
  expect(clampSideWidth(90)).toBe(44);
  expect(clampSideWidth(30.04)).toBe(30);
  expect(clampSideWidth(40, 60)).toBe(28.8);
});

const key = (name: string, modifiers: { shiftKey?: boolean; altKey?: boolean; ctrlKey?: boolean; metaKey?: boolean } = {}) =>
  ({ key: name, shiftKey: false, altKey: false, ctrlKey: false, metaKey: false, ...modifiers });

test('keys move by 1rem, 4rem with Shift, and jump to the ends; the column is on the right so left widens', () => {
  expect(sideWidthForKey(key('ArrowLeft'), 27)).toBe(28);
  expect(sideWidthForKey(key('ArrowRight'), 27)).toBe(26);
  expect(sideWidthForKey(key('ArrowLeft', { shiftKey: true }), 27)).toBe(31);
  expect(sideWidthForKey(key('ArrowRight', { shiftKey: true }), 27)).toBe(23);
  expect(sideWidthForKey(key('ArrowRight', { shiftKey: true }), 24)).toBe(22);
  expect(sideWidthForKey(key('ArrowLeft', { shiftKey: true }), 42)).toBe(44);
  expect(sideWidthForKey(key('Home'), 30)).toBe(22);
  expect(sideWidthForKey(key('End'), 30)).toBe(44);
  expect(sideWidthForKey(key('End'), 30, 60)).toBe(28.8);
  expect(sideWidthForKey(key('Enter'), 30)).toBeUndefined();
});

test('keys with Alt, Ctrl or Meta are left to the browser', () => {
  for (const modifier of ['altKey', 'ctrlKey', 'metaKey'] as const) {
    for (const name of ['ArrowLeft', 'ArrowRight', 'Home', 'End']) expect(sideWidthForKey(key(name, { [modifier]: true }), 27)).toBeUndefined();
  }
});

test('the stylesheet fallback and 48% cap match the constants here', () => {
  const css = readFileSync(fileURLToPath(new URL('../../app/pages-tree.css', import.meta.url)), 'utf8');
  const match = /--tree-side:\s*min\(var\(--tree-side-preference,\s*([\d.]+)rem\),\s*([\d.]+)%\)/.exec(css);
  expect(match).not.toBeNull();
  expect(Number(match![1])).toBe(SIDE_DEFAULT);
  expect(Number(match![2])).toBe(Math.round(SIDE_MAX_SHARE * 100));
});

test('a stored width is only used when it is a number inside the range', () => {
  expect(parseSideWidth('31.5')).toBe(31.5);
  for (const raw of [null, '', ' ', 'wide', 'NaN', '10', '99', '-30', 'Infinity']) expect(parseSideWidth(raw)).toBe(SIDE_DEFAULT);
});

test('reads and writes survive storage that throws', () => {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => { store.set(key, value); } });
  expect(readSideWidth()).toBe(SIDE_DEFAULT);
  writeSideWidth(33);
  expect([...store.keys()]).toEqual(['pobr-v2-tree-side-width']);
  expect(readSideWidth()).toBe(33);
  vi.stubGlobal('localStorage', { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('full'); } });
  expect(readSideWidth()).toBe(SIDE_DEFAULT);
  expect(() => writeSideWidth(30)).not.toThrow();
});
