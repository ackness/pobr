import { describe, expect, test } from 'vitest';
import { LANGS, fallbackChain, gameTextFallback, lookup, messages, translated } from '.';
import { statNameLabel } from '../lib/i18n';

describe('interface locale files', () => {
  test('each language resolves through its declared fallbacks, then English', () => {
    expect(LANGS).toEqual(['zh-CN', 'zh-TW', 'en-US']);
    expect(fallbackChain('zh-TW')).toEqual(['zh-TW', 'zh-CN', 'en-US']);
    expect(fallbackChain('zh-CN')).toEqual(['zh-CN', 'en-US']);
    expect(fallbackChain('en-US')).toEqual(['en-US']);
    expect(messages('zh-TW', 'language').name).toBe('繁體中文');
  });

  test('the game-text fallback is declared apart from the interface fallback', () => {
    expect(gameTextFallback('zh-CN')).toEqual(['zh-TW']);
    expect(gameTextFallback('zh-TW')).toEqual(['zh-CN']);
    expect(gameTextFallback('en-US')).toEqual([]);
    // Interface copy never falls back to the other script.
    expect(lookup('zh-CN', 'language', 'name')).toBe('简体中文');
  });

  test('data-derived keys never resolve to Object members', () => {
    expect(lookup('en-US', 'slot', 'constructor')).toBeUndefined();
    expect(lookup('en-US', 'slot', 'ring1')).toBe('Ring 1');
  });

  test('statExtra only translates; English stat names keep reading detail labels', () => {
    const ids = Object.keys(messages('en-US', 'statExtra'));
    expect(ids.length).toBeGreaterThan(0);
    for (const id of ids) {
      expect(translated('en-US', 'statExtra', id)).toBeUndefined();
      expect(translated('zh-CN', 'statExtra', id)).toBe(messages('zh-CN', 'statExtra')[id as never]);
    }
    const shared = ids.find(id => lookup('en-US', 'detail', id) !== undefined && statNameLabel('en-US', id) === lookup('en-US', 'detail', id));
    expect(shared).toBeDefined();
  });
});
