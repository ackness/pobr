import { describe, expect, test } from 'vitest';
import { feasibleOf, rankVariants, scoreOf, subsetCount, subsets } from './optimize';
import type { VariantStats } from '../api/types';

describe('scoreOf / feasibleOf', () => {
  test('plain objective reads the stat', () => {
    expect(scoreOf({ TotalDPS: 1000 }, { stat: 'TotalDPS', constraints: [] })).toBe(1000);
  });

  test('ratio objective divides, and degrades to numerator on zero denominator', () => {
    const obj = { stat: 'TotalDPS', per: 'ManaCost', constraints: [] };
    expect(scoreOf({ TotalDPS: 1000, ManaCost: 50 }, obj)).toBe(20);
    expect(scoreOf({ TotalDPS: 1000, ManaCost: 0 }, obj)).toBe(1000);
  });

  test('constraints check min and max', () => {
    const obj = {
      stat: 'TotalDPS',
      constraints: [{ stat: 'ActionRate', min: 10 }],
    };
    expect(feasibleOf({ ActionRate: 12 }, obj)).toBe(true);
    expect(feasibleOf({ ActionRate: 8 }, obj)).toBe(false);
  });
});

describe('rankVariants', () => {
  const v = (index: number, stats: Record<string, number>, error: string | null = null) =>
    ({ index, label: null, stats, error }) as VariantStats;

  test('feasible first, then score desc, errors last', () => {
    const obj = { stat: 'TotalDPS', constraints: [{ stat: 'ActionRate', min: 10 }] };
    const ranked = rankVariants(
      [
        v(0, { TotalDPS: 900, ActionRate: 12 }),
        v(1, { TotalDPS: 2000, ActionRate: 5 }), // 分高但不可行
        v(2, {}, 'boom'), // 错误沉底
        v(3, { TotalDPS: 1100, ActionRate: 11 }),
      ],
      obj,
    );
    expect(ranked.map((r) => r.index)).toEqual([3, 0, 1, 2]);
  });
});

describe('subsets / subsetCount', () => {
  test('enumerates non-empty subsets up to maxSize, small first', () => {
    const out = subsets(['a', 'b', 'c'], 2);
    expect(out.length).toBe(6); // C(3,1) + C(3,2)
    expect(out.slice(0, 3).every((s) => s.length === 1)).toBe(true);
    expect(subsetCount(3, 2)).toBe(6);
    expect(subsetCount(8, 3)).toBe(8 + 28 + 56);
  });
});
