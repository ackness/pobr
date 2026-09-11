import { expect, test } from 'vitest';
import type { CalculateBuildRequest, GemInput, SocketGroupInput } from '../api/types';
import type { EvaluateOptions } from './optimize';
import { eligibleSupports, lineageAvailable, optimizeSupports, sameSupportFamily, supportSetCompatible, supportVariant,
  typeExpressionMatches, type SupportMetadata } from './supportOptimizer';

const gem = (id: string, extra: Partial<SupportMetadata> = {}): SupportMetadata => ({
  skill_id: id, name: id, family: id, max_level: 1, level_requirements: [0], is_support: true,
  compatibility_known: true, require_skill_types: ['Spell'], exclude_skill_types: [], add_skill_types: [],
  ...extra,
});
const active = gem('Fireball', { is_support: false, skill_types: ['Spell', 'Projectile'], max_level: 20 });
const input = (id: string): GemInput => ({ skill_id: id, level: 1, quality: 0 });
const group = (ids: string[] = []): SocketGroupInput => ({ enabled: true, gems: [input('Fireball'), ...ids.map(input)] });
const objective = { stat: 'TotalDPS', constraints: [] };

test('postfix support expressions retain logical order and the implicit OR between results', () => {
  expect(typeExpressionMatches(['Attack', 'Bow', 'AND'], new Set(['Spell', 'Bow']))).toBe(false);
  expect(typeExpressionMatches(['Spell', 'Channel', 'NOT', 'AND'], new Set(['Spell']))).toBe(true);
  expect(typeExpressionMatches(['Spell', 'Channel', 'NOT', 'AND'], new Set(['Spell', 'Channel']))).toBe(false);
  expect(typeExpressionMatches(['Attack', 'Spell'], new Set(['Spell']))).toBe(true);
  expect(typeExpressionMatches(['AND'], new Set(['Spell']))).toBe(false);
});

test('automatic pool includes conditional pairs but rejects wrong types, level and unknown metadata', () => {
  const adds = gem('AddsMinion', { add_skill_types: ['Minion'] });
  const needs = gem('NeedsMinion', { require_skill_types: ['Minion'] });
  const catalog = [active, adds, needs,
    gem('AttackOnly', { require_skill_types: ['Attack'] }),
    gem('TooHigh', { level_requirements: [60] }), gem('Unknown', { compatibility_known: false }),
    gem('Lineage', { is_lineage: true })];
  const result = eligibleSupports(group(), catalog, 50, false);
  expect(result.gems.map(gem => gem.skill_id)).toEqual(['AddsMinion', 'NeedsMinion']);
  expect(result).toMatchObject({ unknown: 1, levelBlocked: 1, incompatible: 1 });
  expect(supportSetCompatible(group(), [input('NeedsMinion'), input('AddsMinion')], catalog)).toBe(true);
  expect(supportSetCompatible(group(), [input('NeedsMinion')], catalog)).toBe(false);
});

test('compatibility rechecks exclusions after additions and overlapping families, including item-granted skills', () => {
  const catalog = [active, gem('AddsMinion', { add_skill_types: ['Minion'] }),
    gem('NoMinion', { exclude_skill_types: ['Minion'] }), gem('GemOnly', { support_gems_only: true }),
    gem('Arrow', { families: ['Arrow'] }), gem('Alignment', { families: ['Alignment', 'Arrow'] })];
  expect(supportSetCompatible(group(), [input('NoMinion'), input('AddsMinion')], catalog)).toBe(false);
  expect(supportSetCompatible(group(), [input('Arrow'), input('Alignment')], catalog)).toBe(false);
  expect(sameSupportFamily(catalog[4], catalog[5])).toBe(true);
  expect(supportSetCompatible({ ...group(), source: 'Weapon 1' }, [input('GemOnly')], catalog)).toBe(false);
  expect(supportSetCompatible(group(), [input('GemOnly')], catalog)).toBe(true);
});

test('a support replacement preserves active gems and every other group, including weapon bindings', () => {
  const request = { socket_groups: [group(['Old']), { ...group(), weapon_set: 2 as const, enabled: false }] };
  const variant = supportVariant(request, 0, [input('New')], [active, gem('Old'), gem('New')]);
  expect(variant.socket_groups![0].gems.map(gem => gem.skill_id)).toEqual(['Fireball', 'New']);
  expect(variant.socket_groups![1]).toEqual(request.socket_groups[1]);
  expect(request.socket_groups[0].gems[1].skill_id).toBe('Old');
});

function evaluator(score: (ids: string[]) => number, options: { unsupported?: string; abort?: boolean } = {}) {
  return async ({ request, variants }: EvaluateOptions) => {
    const measure = (groups: SocketGroupInput[] | undefined) => {
      const ids = groups?.[0]?.gems.slice(1).map(gem => gem.skill_id) ?? [];
      return { TotalDPS: score(ids), TotalEHP: ids.includes('Fragile') ? 50 : 1000 };
    };
    return { baseline: measure(request.socket_groups), aborted: Boolean(options.abort),
      results: variants.map((variant, index) => ({ index, label: null, error: null,
        stats: measure(variant.socket_groups ?? request.socket_groups),
        unsupported: ['Existing unmodeled Guard', ...(options.unsupported && variant.socket_groups?.[0].gems.some(gem => gem.skill_id === options.unsupported)
          ? ['Unmodeled candidate effect'] : [])] })) };
  };
}

function exhaustiveSupports(catalog: SupportMetadata[], capacity: number, score: (ids: string[]) => number) {
  const supports = catalog.filter(gem => gem.is_support);
  let best = 0;
  let evaluations = 0;
  for (let mask = 0; mask < 2 ** supports.length; mask++) {
    const chosen = supports.filter((_, index) => (mask & 2 ** index) !== 0);
    if (chosen.length > capacity || !supportSetCompatible(group(), chosen.map(gem => input(gem.skill_id)), catalog)) continue;
    best = Math.max(best, score(chosen.map(gem => gem.skill_id)));
    evaluations++;
  }
  return { best, evaluations };
}

test('bounded combinations match exhaustive optimum and beat isolated support ranking on interaction fixtures', async () => {
  const scenarios = [
    { name: 'neutral pair synergy', supports: [gem('A'), gem('B'), gem('Greedy'), gem('Weak')],
      score: (ids: string[]) => 100 + (ids.includes('A') && ids.includes('B') ? 200 : 0) + (ids.includes('Greedy') ? 80 : 0) },
    { name: 'type-enabling pair', supports: [gem('A', { add_skill_types: ['Minion'] }),
      gem('B', { require_skill_types: ['Minion'] }), gem('Greedy')],
      score: (ids: string[]) => 100 + (ids.includes('A') && ids.includes('B') ? 160 : 0) + (ids.includes('Greedy') ? 70 : 0) },
  ];
  for (const scenario of scenarios) {
    const catalog = [active, ...scenario.supports];
    const request: CalculateBuildRequest = { character: { level: 80 }, socket_groups: [group()] };
    const result = await optimizeSupports({ request, groupIndex: 0, catalog, capacity: 2, objective,
      evaluate: evaluator(scenario.score) });
    const oracle = exhaustiveSupports(catalog, 2, scenario.score);
    const isolated = scenario.supports.filter(gem => supportSetCompatible(group(), [input(gem.skill_id)], catalog))
      .sort((a, b) => scenario.score([b.skill_id]) - scenario.score([a.skill_id])).slice(0, 2).map(gem => gem.skill_id);
    const oldScore = scenario.score(isolated);
    const newScore = result.plans[0].stats.TotalDPS;
    expect(newScore, scenario.name).toBe(oracle.best);
    expect(newScore, scenario.name).toBeGreaterThan(oldScore);
    expect(result.evaluated).toBeLessThanOrEqual(oracle.evaluations);
    // The applied gem array is exactly the evaluated variant, so its score is reproducible.
    const applied = result.plans[0].variant.socket_groups![0].gems.slice(1).map(gem => gem.skill_id);
    expect(scenario.score(applied)).toBe(newScore);
  }
});

test('filled sockets are replaced and existing unsupported lines do not hide known improvements', async () => {
  const catalog = [active, gem('Old'), gem('New'), gem('Unmodeled'), gem('Fragile')];
  const request = { character: { level: 50 }, socket_groups: [group(['Old'])] };
  const result = await optimizeSupports({ request, groupIndex: 0, catalog, capacity: 1,
    objective: { stat: 'TotalDPS', constraints: [{ stat: 'TotalEHP', min: 1000 }] },
    evaluate: evaluator(ids => ids.includes('Unmodeled') ? 1000 : ids.includes('Fragile') ? 900 : ids.includes('New') ? 200 : 100,
      { unsupported: 'Unmodeled' }) });
  expect(result.plans[0].supports.map(gem => gem.skill_id)).toEqual(['New']);
  expect(result.plans.every(plan => plan.supports.length <= 1)).toBe(true);
  expect(result.plans.some(plan => plan.supports.some(gem => ['Unmodeled', 'Fragile'].includes(gem.skill_id)))).toBe(false);
  expect(result.unmodeled).toBeGreaterThan(0);
});

test('cancellation rejects partially evaluated results', async () => {
  await expect(optimizeSupports({ request: { socket_groups: [group()] }, groupIndex: 0,
    catalog: [active, gem('New')], capacity: 2, objective, evaluate: evaluator(() => 100, { abort: true }) })).rejects.toThrow('cancelled');
});


test('lineage supports respect the default copy limit across skill and weapon groups', () => {
  const lineage = gem('Lineage', { is_lineage: true });
  const groups = [group(), { ...group(['Lineage']), enabled: false, weapon_set: 2 as const }];
  expect(lineageAvailable(lineage, groups, 0)).toBe(false);
  expect(lineageAvailable(lineage, groups, 1)).toBe(true);
  expect(lineageAvailable(lineage, groups, 0, 2)).toBe(true);
  expect(lineageAvailable(gem('Lineage'), groups, 0)).toBe(true);
});


test('candidate screening does not let a different optional support suppress a legal conditional pair', () => {
  const catalog = [active, gem('AddsMinion', { add_skill_types: ['Minion'] }),
    gem('AddsArea', { add_skill_types: ['Area'] }),
    gem('MinionWithoutArea', { require_skill_types: ['Minion'], exclude_skill_types: ['Area'] })];
  expect(eligibleSupports(group(), catalog, 50).gems.map(gem => gem.skill_id)).toContain('MinionWithoutArea');
  expect(supportSetCompatible(group(), [input('AddsMinion'), input('MinionWithoutArea')], catalog)).toBe(true);
  expect(supportSetCompatible(group(), [input('AddsArea'), input('AddsMinion'), input('MinionWithoutArea')], catalog)).toBe(false);
});
