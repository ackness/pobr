import { describe, expect, test } from 'vitest';
import { formatStatValue, statMap, STAT_SECTIONS } from './statDisplay';

describe('formatStatValue', () => {
  test('int 取整加千分位', () => {
    expect(formatStatValue(10005.12, 'int')).toBe('10,005');
  });

  test('percent 保留一位', () => {
    expect(formatStatValue(75, 'percent')).toBe('75%');
    expect(formatStatValue(46.832, 'percent')).toBe('46.8%');
  });

  test('float2 大数退化为整数展示', () => {
    expect(formatStatValue(4143.7479, 'float2')).toBe('4,144');
    expect(formatStatValue(4.59, 'float2')).toBe('4.59');
  });

  test('null / 非有限值显示 ∞（ChaosMaxHit 免疫语义）', () => {
    expect(formatStatValue(null, 'int')).toBe('∞');
    expect(formatStatValue(Number.POSITIVE_INFINITY, 'int')).toBe('∞');
  });
});

describe('statMap', () => {
  test('id 索引', () => {
    const map = statMap([{ id: 'Life', value: 1, category: 'Resource' }]);
    expect(map.get('Life')).toBe(1);
  });
});

describe('STAT_SECTIONS', () => {
  test('行 id 全局唯一（侧边栏不重复展示同一字段）', () => {
    const ids = STAT_SECTIONS.flatMap((s) => s.rows.map((r) => r.id));
    expect(new Set(ids).size).toBe(ids.length);
  });

  test('双语标签齐全', () => {
    for (const section of STAT_SECTIONS) {
      for (const row of section.rows) {
        expect(row.label['en-US']).toBeTruthy();
        expect(row.label['zh-TW']).toBeTruthy();
      }
    }
  });
});
