import { afterEach, expect, test } from 'vitest';
import { gameTextChain, lineTemplateLocale, setGameLocales } from './gameLocales';
import { localizedName } from './chinese';

afterEach(() => setGameLocales([]));

test('without a data pack, game text falls back through the interface files only', () => {
  expect(gameTextChain('en-US')).toEqual([]);
  expect(gameTextChain('zh-CN')).toEqual(['zh-CN', 'zh-TW']);
  expect(gameTextChain('zh-TW')).toEqual(['zh-TW', 'zh-CN']);
  expect(lineTemplateLocale('zh-TW')).toBeUndefined();
});

test("the data pack's declared fallbacks come before the interface ones", () => {
  setGameLocales([
    { code: 'zh-TW', fallback: ['xx'], chain: ['zh-TW', 'xx'], templates: false },
    { code: 'xx', fallback: [], chain: ['xx'], templates: true },
    { code: 'zh-CN', fallback: [], chain: ['zh-CN'], templates: true },
  ]);
  expect(gameTextChain('zh-TW')).toEqual(['zh-TW', 'xx', 'zh-CN']);
  expect(lineTemplateLocale('zh-TW')).toBe('xx');
  expect(lineTemplateLocale('zh-CN')).toBe('zh-CN');
});

test('a missing name comes from the next locale in the chain, in the asked script', () => {
  expect(localizedName('zh-TW', 'Fire', '火焰', '烈焰')).toBe('烈焰');
  expect(localizedName('zh-TW', 'Sapphire Ring', '蓝玉戒指', ' ')).toBe('藍玉戒指');
  expect(localizedName('zh-CN', 'Sapphire Ring', null, '藍玉戒指')).toBe('蓝玉戒指');
  expect(localizedName('en-US', 'Fire', '火焰', '烈焰')).toBe('Fire');
  expect(localizedName('zh-CN', 'Fire', null, null)).toBe('Fire');
});
