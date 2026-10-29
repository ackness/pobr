import { describe, expect, it } from 'vitest';
import type { CalculateBuildResponse } from '../api/types';
import { deriveFindings } from './findings';

function calc(values: Record<string, number>, extra: Partial<CalculateBuildResponse> = {}): CalculateBuildResponse {
  return {
    stats: Object.entries(values).map(([id, value]) => ({ id, value, category: 'Resistance' as const })),
    unsupported_modifiers: [], breakdowns: {}, main_skill: { group_index: 0, active_skill_index: 1, skill_id: 'x', hit_damage: [], hit_dps: 1, dot_dps: 0, combined_dps: 1 },
    item_errors: [], ...extra,
  };
}
const kinds = (response: CalculateBuildResponse | null, hasSkills = true) => deriveFindings(response, hasSkills).map(item => item.kind);

describe('deriveFindings', () => {
  it('reports nothing before a calculation exists', () => {
    expect(deriveFindings(null, true)).toEqual([]);
  });

  it('flags only the elements that fall short of the resistance cap', () => {
    const found = deriveFindings(calc({ FireResist: 75, ColdResist: 60, LightningResist: -10, ChaosResist: 0 }), true);
    expect(found.map(item => item.kind === 'resist' ? [item.element, item.missing, item.severity] : null)).toEqual([
      ['Lightning', 85, 'bad'], ['Cold', 15, 'warn'],
    ]);
  });

  it('does not call a raised maximum resistance a shortfall', () => {
    expect(kinds(calc({ FireResist: 78, FireResistOverCap: 12, ColdResist: 75, LightningResist: 75 }))).toEqual([]);
  });

  it('reports negative chaos resistance but not zero', () => {
    expect(kinds(calc({ FireResist: 75, ColdResist: 75, LightningResist: 75, ChaosResist: 0 }))).toEqual([]);
    expect(kinds(calc({ FireResist: 75, ColdResist: 75, LightningResist: 75, ChaosResist: -30 }))).toEqual(['resist']);
  });

  it('reports over-committed Spirit and unmet attribute requirements first', () => {
    const found = deriveFindings(calc({ SpiritUnreserved: -20, Str: 80, ReqStr: 100, Dex: 90, ReqDex: 90, HitChance: 90 }), true);
    expect(found.map(item => item.kind)).toEqual(['spirit', 'attribute', 'hit-chance']);
    expect(found[1]).toMatchObject({ attribute: 'Str', have: 80, need: 100 });
  });

  it('ignores a hit chance of zero (spells and skills without a hit)', () => {
    expect(kinds(calc({ HitChance: 0 }))).toEqual([]);
  });

  it('surfaces data-quality problems: item errors, unsupported lines, no main skill', () => {
    const found = deriveFindings(calc({}, { main_skill: null, item_errors: [{ slot: 'Helmet', message: 'x' }], unsupported_modifiers: ['a', 'b'] }), true);
    expect(found.map(item => item.kind)).toEqual(['no-main-skill', 'item-errors', 'unsupported']);
    expect(found[1]).toMatchObject({ slots: ['Helmet'] });
    expect(found[2]).toMatchObject({ count: 2 });
  });

  it('does not demand a main skill from a build that has no skills', () => {
    expect(kinds(calc({}, { main_skill: null }), false)).toEqual([]);
  });

  it('warns when recovery is lower than the build’s own drain', () => {
    expect(kinds(calc({ TotalNetRegen: -12 }))).toEqual(['net-regen']);
    expect(kinds(calc({ TotalNetRegen: 0 }))).toEqual([]);
  });
});
