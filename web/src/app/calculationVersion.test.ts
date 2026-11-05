import { expect, it } from 'vitest';
import { calculationVersion } from './calculationVersion';

it.each([
  ['en-US', 'Data 0.5.5.5 · Engine 0.0.45 · Calculation 77 ms'],
  ['zh-CN', '数据 0.5.5.5 · 引擎 0.0.45 · 计算 77 ms'],
  ['zh-TW', '資料 0.5.5.5 · 引擎 0.0.45 · 計算 77 ms'],
] as const)('labels versions and elapsed time in the same order in %s', (lang, expected) => {
  expect(calculationVersion(lang, '0.5.5.5', '0.0.45', 77.1)).toBe(expected);
  expect(calculationVersion(lang, null, undefined, null)).toBe('');
});
