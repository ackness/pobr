import { describe, expect, it } from 'vitest';
import type { StatChange } from '../motion/statChanges';
import { summarizeChanges } from './changeSummary';

const change = (id: string, percent: number | null, delta = percent ?? 0): StatChange => ({ id, before: 100, after: 100 + delta, delta, percent, tone: delta >= 0 ? 'better' : 'worse' });
const map = (...rows: StatChange[]) => new Map(rows.map(row => [row.id, row]));

describe('summarizeChanges', () => {
  it('puts headline stats first, in a fixed order, then the biggest relative moves', () => {
    const result = summarizeChanges(map(change('Armour', 40), change('Life', 2), change('CombinedDPS', 5), change('Evasion', -60)), 10);
    expect(result.map(row => row.id)).toEqual(['CombinedDPS', 'Life', 'Evasion', 'Armour']);
  });
  it('caps the list and copes with nothing changing', () => {
    expect(summarizeChanges(map(...Array.from({ length: 12 }, (_, i) => change(`X${i}`, i + 1))), 6)).toHaveLength(6);
    expect(summarizeChanges(new Map())).toEqual([]);
  });
});
