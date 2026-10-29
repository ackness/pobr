import { expect, test } from 'vitest';
import { defaultMetric } from './metricSelection';

test('starts with damage for a selected skill and resources without one', () => {
  const stats = [{ id: 'Life', value: 100 }, { id: 'TotalDPS', value: 200 }, { id: 'CombinedDPS', value: 250 }];
  expect(defaultMetric(stats, true)).toBe('CombinedDPS');
  expect(defaultMetric(stats, false)).toBe('Life');
});

test('keeps measured zero but skips missing and unbounded results', () => {
  expect(defaultMetric([{ id: 'CombinedDPS', value: 0 }, { id: 'Life', value: 100 }], true)).toBe('CombinedDPS');
  expect(defaultMetric([{ id: 'CombinedDPS', value: null }, { id: 'Life', value: Infinity }, { id: 'Mana', value: 0 }], true)).toBe('Mana');
  expect(defaultMetric([{ id: 'Custom', value: 20 }], true)).toBe('Custom');
  expect(defaultMetric([{ id: 'Life', value: null }], false)).toBeNull();
  expect(defaultMetric([], true)).toBeNull();
});
