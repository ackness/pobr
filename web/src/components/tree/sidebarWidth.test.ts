import { afterEach, expect, test, vi } from 'vitest';
import { SIDE_DEFAULT, clampSideWidth, parseSideWidth, readSideWidth, sideWidthForKey, sideWidthLimits, writeSideWidth } from './sidebarWidth';

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

test('keys move by 1rem, 4rem with Shift, and jump to the ends; the column is on the right so left widens', () => {
  expect(sideWidthForKey('ArrowLeft', false, 27)).toBe(28);
  expect(sideWidthForKey('ArrowRight', false, 27)).toBe(26);
  expect(sideWidthForKey('ArrowLeft', true, 27)).toBe(31);
  expect(sideWidthForKey('ArrowRight', true, 27)).toBe(23);
  expect(sideWidthForKey('ArrowRight', true, 24)).toBe(22);
  expect(sideWidthForKey('ArrowLeft', true, 42)).toBe(44);
  expect(sideWidthForKey('Home', false, 30)).toBe(22);
  expect(sideWidthForKey('End', false, 30)).toBe(44);
  expect(sideWidthForKey('End', false, 30, 60)).toBe(28.8);
  expect(sideWidthForKey('Enter', false, 30)).toBeUndefined();
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
