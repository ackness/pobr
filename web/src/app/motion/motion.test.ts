import { beforeAll, describe, expect, it } from 'vitest';
import type { DisplayCatalogEntry, DisplayStatValue } from '../../api/types';
import catalog from '../../api/generated/display_catalog.json';
import { setDisplayCatalog } from '../../lib/statDisplay';
import { computeStatChanges } from './statChanges';
import { statPolarity } from './statPolarity';
import { easeOutCubic, tween } from './tween';

const stat = (id: string, value: number | null): DisplayStatValue => ({ id, value, category: 'Offence' });

beforeAll(() => setDisplayCatalog(catalog as DisplayCatalogEntry[]));

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
  it('honors every explicit catalog direction', () => {
    for (const entry of catalog) expect(statPolarity(entry.id), entry.id).toBe(entry.higher_is_better === false ? -1 : entry.higher_is_better === null ? 0 : 1);
  });
  it('treats damage, defences and resources as higher-is-better', () => {
    for (const id of ['CombinedDPS', 'Life', 'TotalEHP', 'FireResist', 'SpiritUnreserved', 'Str']) expect(statPolarity(id), id).toBe(1);
  });
  it('keeps unspecified catalog directions neutral', () => {
    for (const id of ['Flask1Duration', 'Flask1Recovery']) expect(statPolarity(id), id).toBe(0);
  });
});

describe('computeStatChanges', () => {
  it('has nothing to say without a previous result', () => {
    expect(computeStatChanges(null, [stat('Life', 100)]).size).toBe(0);
  });
  it('reports only what moved and judges it by polarity', () => {
    const changes = computeStatChanges(
      [stat('CombinedDPS', 1000), stat('ManaCost', 20), stat('Life', 500), stat('Flask1Duration', 4)],
      [stat('CombinedDPS', 1100), stat('ManaCost', 25), stat('Life', 500), stat('Flask1Duration', 5)]);
    expect([...changes.keys()].sort()).toEqual(['CombinedDPS', 'Flask1Duration', 'ManaCost']);
    expect(changes.get('CombinedDPS')).toMatchObject({ delta: 100, percent: 10, tone: 'better' });
    expect(changes.get('ManaCost')?.tone).toBe('worse');
    expect(changes.get('Flask1Duration')?.tone).toBe('neutral');
  });
  it('ignores float noise and unbounded values', () => {
    expect(computeStatChanges([stat('Life', 100), stat('ChaosMaxHit', null)], [stat('Life', 100.001), stat('ChaosMaxHit', 5)]).size).toBe(0);
  });
});
