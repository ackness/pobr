import { describe, expect, it } from 'vitest';
import type { SkillMetricsEntry } from '../../../api/types';
import { barShare, groupDps, topDps } from './dps';

const entry = (group_index: number, active_skill_index: number, combined_dps: number | null, status: SkillMetricsEntry['status'] = 'calculated'): SkillMetricsEntry => ({
  group_index, active_skill_index, skill_id: `skill${group_index}`, status,
  metrics: combined_dps === null ? null : { hit_dps: combined_dps, dot_dps: 0, combined_dps, average_hit: 0, action_rate: 0, mana_cost: 0, life_cost: 0, cooldown: 0 },
  unsupported_modifiers: [], error: null,
});

describe('groupDps', () => {
  it('reads the effect each group has selected, defaulting to the first', () => {
    const rows = groupDps([entry(0, 1, 100), entry(0, 2, 900), entry(1, 1, 50)], [{ main_active_skill: 2 }, {}]);
    expect(rows.get(0)).toEqual({ kind: 'damage', dps: 900 });
    expect(rows.get(1)).toEqual({ kind: 'damage', dps: 50 });
  });

  it('tells a skill with no damage apart from one that has no figure', () => {
    const rows = groupDps([entry(0, 1, 0), entry(1, 1, null, 'unavailable')], [{}, {}, {}]);
    expect(rows.get(0)).toEqual({ kind: 'none' });
    expect(rows.get(1)).toEqual({ kind: 'unavailable' });
    expect(rows.get(2)).toEqual({ kind: 'unavailable' });
  });

  it('does not invent a figure for a non-finite result', () => {
    expect(groupDps([entry(0, 1, Number.NaN)], [{}]).get(0)).toEqual({ kind: 'unavailable' });
    expect(groupDps([entry(0, 1, Number.POSITIVE_INFINITY)], [{}]).get(0)).toEqual({ kind: 'unavailable' });
  });

  it('ignores a calculated entry that carries no metrics', () => {
    expect(groupDps([entry(0, 1, null)], [{}]).get(0)).toEqual({ kind: 'unavailable' });
  });
});

describe('topDps and barShare', () => {
  it('scales every bar to the strongest damaging group', () => {
    const rows = groupDps([entry(0, 1, 200), entry(1, 1, 50), entry(2, 1, 0)], [{}, {}, {}]);
    expect(topDps(rows)).toBe(200);
    expect(barShare(200, 200)).toBe(1);
    expect(barShare(50, 200)).toBe(0.25);
  });

  it('keeps a tiny contribution visible and an absent one empty', () => {
    expect(barShare(1, 1_000_000)).toBe(0.02);
    expect(barShare(0, 100)).toBe(0);
    expect(barShare(10, 0)).toBe(0);
    expect(topDps(new Map())).toBe(0);
  });
});
