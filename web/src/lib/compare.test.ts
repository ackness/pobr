import { describe, expect, test } from 'vitest';
import { diffStats } from './compare';
import type { DisplayStatValue } from '../api/types';

const stats = (pairs: [string, number | null][]): DisplayStatValue[] =>
  pairs.map(([id, value]) => ({ id, value, category: 'Offence' }));

describe('diffStats', () => {
  test('返回非零差异并按幅度排序', () => {
    const before = stats([
      ['TotalDPS', 100],
      ['Life', 50],
      ['Mana', 40],
    ]);
    const after = stats([
      ['TotalDPS', 150],
      ['Life', 45],
      ['Mana', 40],
    ]);
    const diffs = diffStats(before, after);
    expect(diffs.map((d) => d.id)).toEqual(['TotalDPS', 'Life']);
    expect(diffs[0].delta).toBe(50);
    expect(diffs[1].delta).toBe(-5);
  });

  test('忽略 null（∞）字段与浮点噪声', () => {
    const before = stats([
      ['ChaosMaxHit', null],
      ['Life', 50],
    ]);
    const after = stats([
      ['ChaosMaxHit', 999],
      ['Life', 50.001],
    ]);
    expect(diffStats(before, after)).toEqual([]);
  });
});
