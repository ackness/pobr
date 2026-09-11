import { expect, test } from 'vitest';
import { rankUpgradePositions, type PositionAnalysis } from './tradePriority';
import { scoreOf, minimumDeficit, type Objective } from './optimize';
const baseline = { TotalDPS: 100, TotalEHP: 1000, Life: 500, FireResist: 75, ColdResist: 20, LightningResist: 73 };
const objective: Objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [{ stat: 'TotalEHP', min: 1000 }] };
function result(patch: Record<string, number>): PositionAnalysis {
  const stats = { ...baseline, ...patch };
  return { weights: { baseline, weighted: [], evaluated: 10, limited: true, minimumWeight: 0, unsupported: [],
    combinations: [{ mods: [], text: 'Synthetic reference', stats, score: scoreOf(stats, objective) }] } };
}
test('balanced ranking values relative DPS and EHP gains equally and rejects lost EHP', () => {
  const ranked = rankUpgradePositions({ ring: result({ TotalDPS: 121 }), helmet: result({ TotalEHP: 1210 }),
    fragile: result({ TotalDPS: 1000, TotalEHP: 900 }), unchanged: result({}) }, objective);
  expect(ranked.map(entry => entry.slot)).toEqual(['helmet', 'ring']);
  expect(ranked[0].gainPercent).toBeCloseTo(10);
  expect(ranked[1].gainPercent).toBeCloseTo(10);
});
test('resistance priority accepts partial progress before all three resists reach the target', () => {
  const resist: Objective = { ...objective, softMinimums: ['FireResist', 'ColdResist', 'LightningResist'].map(stat => ({ stat, min: 75 })) };
  const ranked = rankUpgradePositions({ damage: result({ TotalDPS: 500 }), cold: result({ ColdResist: 65 }),
    capped: result({ ColdResist: 75, LightningResist: 75 }), worse: result({ ColdResist: 10 }),
    fragile: result({ ColdResist: 75, LightningResist: 75, TotalEHP: 500 }) }, resist);
  expect(ranked.map(entry => entry.slot)).toEqual(['capped', 'cold', 'damage']);
  expect(ranked[1].deficit).toBe(12);
  expect(minimumDeficit({ ...baseline, ColdResist: 120 }, resist)).toBe(2);
});
test('only completed improving references appear; zero baseline never yields infinity', () => {
  const empty = result({ TotalDPS: 20 }); empty.weights!.baseline = { TotalDPS: 0, TotalEHP: 1000 };
  const ranked = rankUpgradePositions({ missing: {}, error: { error: 'Unavailable' }, empty }, objective);
  expect(ranked).toHaveLength(1); expect(ranked[0].gainPercent).toBeUndefined(); expect(Number.isFinite(ranked[0].gain)).toBe(true);
});
