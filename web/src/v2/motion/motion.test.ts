import { describe, expect, it } from 'vitest';
import type { DisplayStatValue } from '../../api/types';
import { computeStatChanges } from './statChanges';
import { statPolarity } from './statPolarity';
import { easeOutCubic, tween } from './tween';

const stat = (id: string, value: number | null): DisplayStatValue => ({ id, value, category: 'Offence' });

describe('tween', () => {
  it('starts at the origin, ends exactly on the target, and never overshoots', () => {
    expect(tween(100, 200, 0)).toBe(100);
    expect(tween(100, 200, 280)).toBe(200);
    expect(tween(100, 200, 9999)).toBe(200);
    let last = 100;
    for (let elapsed = 0; elapsed <= 280; elapsed += 20) { const value = tween(100, 200, elapsed); expect(value).toBeGreaterThanOrEqual(last); expect(value).toBeLessThanOrEqual(200); last = value; }
  });
  it('moves downward as well, and a zero duration is instant', () => {
    expect(tween(200, 100, 140)).toBeLessThan(200);
    expect(tween(200, 100, 140)).toBeGreaterThan(100);
    expect(tween(1, 2, 0, 0)).toBe(2);
  });
  it('eases out: most of the distance is covered early', () => {
    expect(easeOutCubic(0.5)).toBeGreaterThan(0.8);
  });
});

describe('statPolarity', () => {
  it('treats costs, drain, requirements and use times as lower-is-better', () => {
    for (const id of ['ManaCost', 'LifePerSecondCost', 'ESPercentCost', 'TotalBuildDegen', 'ReqStr', 'ReloadTime', 'TrapThrowingTime']) expect(statPolarity(id), id).toBe(-1);
  });
  it('treats damage, defences and resources as higher-is-better', () => {
    for (const id of ['CombinedDPS', 'Life', 'TotalEHP', 'FireResist', 'SpiritUnreserved', 'Str']) expect(statPolarity(id), id).toBe(1);
  });
  it('does not guess about durations and areas', () => {
    for (const id of ['Duration', 'AuraDuration', 'AreaOfEffectRadiusMetres', 'PresenceRadiusMetres']) expect(statPolarity(id), id).toBe(0);
  });
});

describe('computeStatChanges', () => {
  it('has nothing to say without a previous result', () => {
    expect(computeStatChanges(null, [stat('Life', 100)]).size).toBe(0);
  });
  it('reports only what moved and judges it by polarity', () => {
    const changes = computeStatChanges(
      [stat('CombinedDPS', 1000), stat('ManaCost', 20), stat('Life', 500), stat('Duration', 4)],
      [stat('CombinedDPS', 1100), stat('ManaCost', 25), stat('Life', 500), stat('Duration', 5)]);
    expect([...changes.keys()].sort()).toEqual(['CombinedDPS', 'Duration', 'ManaCost']);
    expect(changes.get('CombinedDPS')).toMatchObject({ delta: 100, percent: 10, tone: 'better' });
    expect(changes.get('ManaCost')?.tone).toBe('worse');
    expect(changes.get('Duration')?.tone).toBe('neutral');
  });
  it('ignores float noise and unbounded values', () => {
    expect(computeStatChanges([stat('Life', 100), stat('ChaosMaxHit', null)], [stat('Life', 100.001), stat('ChaosMaxHit', 5)]).size).toBe(0);
  });
});
