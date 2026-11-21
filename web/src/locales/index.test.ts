import { describe, expect, test } from 'vitest';
import { LANGS, fallbackChain, gameTextFallback, lookup, messages, translated } from '.';
import { bindT, bootErrorLabel, statNameLabel, t, type UiKey } from '../lib/i18n';

describe('interface locale files', () => {
  test('each language resolves through its declared fallbacks, then English', () => {
    expect(LANGS.filter(lang => ['zh-CN', 'zh-TW', 'en-US'].includes(lang))).toEqual(['zh-CN', 'zh-TW', 'en-US']);
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

  test('named values are inserted literally and are not re-interpreted as placeholders', () => {
    const tt = bindT('en-US');
    expect(tt('overview.attributeShortfall', { need: 60, have: 45, attribute: '$& {need}' }))
      .toBe('$& {need} 45 of 60 required');
    expect(bindT('zh-TW')('overview.attributeShortfall', { need: 60, have: 45, attribute: '力量' }))
      .toBe('力量不足：45 / 需要 60');
  });

  test('unknown UI keys fall back to the key string without throwing', () => {
    const missing = 'calcs.step.nonexistent_future_op' as UiKey;
    expect(t('en-US', missing)).toBe(missing);
    expect(t('zh-CN', missing, { value: 1 })).toBe(missing);
    expect(t('zh-TW', 'constructor' as UiKey, { value: 1 })).toBe('constructor');
  });

  test('structured backend boot errors localize on the main thread while raw errors stay intact', () => {
    const missingWasm = new Error(JSON.stringify({ bootError: 'wasmNotBuilt', error: 'missing module' }));
    expect(bootErrorLabel('en-US', missingWasm)).toContain('WASM module is not built yet');
    expect(bootErrorLabel('en-US', missingWasm)).toContain('missing module');
    expect(bootErrorLabel('zh-CN', String(missingWasm))).toContain('WASM 模块尚未构建');
    expect(bootErrorLabel('zh-TW', missingWasm)).toContain('WASM 模組尚未建置');

    const mismatch = new Error(JSON.stringify({ bootError: 'schemaMismatch', expected: 4, actual: 3 }));
    expect(bootErrorLabel('en-US', mismatch)).toContain('frontend 4, WASM 3');
    expect(bootErrorLabel('zh-CN', mismatch)).toContain('前端 4，WASM 3');
    expect(bootErrorLabel('zh-TW', mismatch)).toContain('前端 4，WASM 3');

    expect(bootErrorLabel('en-US', new Error('No character classes available'))).toBe('Error: No character classes available');
    expect(bootErrorLabel('zh-CN', 'plain diagnostic')).toBe('plain diagnostic');
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
