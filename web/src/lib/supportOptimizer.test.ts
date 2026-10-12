import { expect, test, vi } from 'vitest';
import type { CalculateBuildRequest, GemInput, SocketGroupInput } from '../api/types';
import type { EvaluateOptions } from './optimize';
import { applySupportPlan, eligibleSupports, lineageAvailable, optimizeSupports, sameSupportFamily, supportSetCompatible, supportSetsCompatible, supportVariant,
  type SupportMetadata } from './supportOptimizer';

// Canned engine decisions for the synthetic search fixtures. Rule semantics and
// fixed-point ordering are tested in Rust; these tests exercise search policy.
const engine = vi.hoisted(() => ({ judge: vi.fn(async (groups: SocketGroupInput[]) => groups.map(group => {
  const ids = group.gems.map(gem => gem.skill_id);
  if (group.source && ids.includes('GemOnly')) return false;
  if (ids.includes('NeedsMinion') && !ids.some(id => ['AddsMinion', 'Enabler'].includes(id))) return false;
  if (ids.includes('NoMinion') && ids.includes('AddsMinion')) return false;
  if (ids.includes('MinionWithoutArea') && (!ids.includes('AddsMinion') || ids.includes('AddsArea'))) return false;
  return true;
})) }));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ supportGroupsCompatible: engine.judge }) }));

const gem = (id: string, extra: Partial<SupportMetadata> = {}): SupportMetadata => ({
  skill_id: id, name: id, family: id, max_level: 1, level_requirements: [0], is_support: true,
  compatibility_known: true, require_skill_types: ['Spell'], exclude_skill_types: [], add_skill_types: [],
  active_effect_ids: extra.is_support === false ? [id] : [],
  ...extra,
});
const active = gem('Fireball', { is_support: false, skill_types: ['Spell', 'Projectile'], max_level: 20 });
const input = (id: string): GemInput => ({ skill_id: id, level: 1, quality: 0 });
const group = (ids: string[] = []): SocketGroupInput => ({ enabled: true, gems: [input('Fireball'), ...ids.map(input)] });
const objective = { stat: 'TotalDPS', constraints: [] };

test('automatic pool includes conditional pairs but rejects wrong types, level and unknown metadata', async () => {
  const adds = gem('AddsMinion', { add_skill_types: ['Minion'] });
  const needs = gem('NeedsMinion', { require_skill_types: ['Minion'] });
  const catalog = [active, adds, needs,
    gem('AttackOnly', { require_skill_types: ['Attack'] }),
    gem('TooHigh', { level_requirements: [60] }), gem('Unknown', { compatibility_known: false }),
    gem('Lineage', { is_lineage: true })];
  const result = eligibleSupports(group(), catalog, 50, false);
  expect(result.gems.map(gem => gem.skill_id)).toEqual(['AddsMinion', 'NeedsMinion']);
  expect(result).toMatchObject({ unknown: 1, levelBlocked: 1, incompatible: 1 });
  expect(await supportSetCompatible(group(), [input('NeedsMinion'), input('AddsMinion')], catalog)).toBe(true);
  expect(await supportSetCompatible(group(), [input('NeedsMinion')], catalog)).toBe(false);
});

test('compatibility rechecks exclusions after additions and overlapping families, including item-granted skills', async () => {
  const catalog = [active, gem('AddsMinion', { add_skill_types: ['Minion'] }),
    gem('NoMinion', { exclude_skill_types: ['Minion'] }), gem('GemOnly', { support_gems_only: true }),
    gem('Arrow', { families: ['Arrow'] }), gem('Alignment', { families: ['Alignment', 'Arrow'] })];
  expect(await supportSetCompatible(group(), [input('NoMinion'), input('AddsMinion')], catalog)).toBe(false);
  expect(await supportSetCompatible(group(), [input('Arrow'), input('Alignment')], catalog)).toBe(false);
  expect(sameSupportFamily(catalog[4], catalog[5])).toBe(true);
  expect(await supportSetCompatible({ ...group(), source: 'Weapon 1' }, [input('GemOnly')], catalog)).toBe(false);
  expect(await supportSetCompatible(group(), [input('GemOnly')], catalog)).toBe(true);
});

test('a support replacement preserves active gems and every other group, including weapon bindings', () => {
  const request = { socket_groups: [group(['Old']), { ...group(), weapon_set: 2 as const, enabled: false }] };
  const variant = supportVariant(request, 0, [input('New')], [active, gem('Old'), gem('New')]);
  expect(variant.socket_groups![0].gems.map(gem => gem.skill_id)).toEqual(['Fireball', 'New']);
  expect(variant.socket_groups![1]).toEqual(request.socket_groups[1]);
  expect(request.socket_groups[0].gems[1].skill_id).toBe('Old');
});

test('support plans preserve the inspected granted effect across reordering and cannot replace it with another skill', async () => {
  const catalog = [active, gem('Hex', { active_effect_ids: ['DoomBlast'] }), gem('New')];
  const original: SocketGroupInput = { enabled: true, weapon_set: 2, main_active_skill: 2,
    gems: [input('Fireball'), input('Hex'), input('Fireball')] };
  const request = { socket_groups: [original] };
  const variant = supportVariant(request, 0, [input('New'), input('Hex')], catalog);
  expect(variant.socket_groups![0].main_active_skill).toBe(3);
  expect(variant.socket_groups![0].gems.map(gem => gem.skill_id)).toEqual(['Fireball', 'Fireball', 'New', 'Hex']);
  expect(applySupportPlan([original], 0, { variant })[0]).toEqual(variant.socket_groups![0]);
  expect(applySupportPlan([original], 0, { variant })[0].weapon_set).toBe(2);
  expect(await supportSetCompatible(original, [input('New')], catalog)).toBe(false);
  expect(() => supportVariant(request, 0, [input('New')], catalog)).toThrow('removes the inspected active effect');
  const duplicate = supportVariant({ socket_groups: [{ ...original, main_active_skill: 3 }] }, 0, [input('New')], catalog);
  expect(duplicate.socket_groups![0].main_active_skill).toBe(2);
  expect(original.main_active_skill).toBe(2);
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

async function exhaustiveSupports(catalog: SupportMetadata[], capacity: number, score: (ids: string[]) => number) {
  const supports = catalog.filter(gem => gem.is_support);
  let best = 0;
  let evaluations = 0;
  for (let mask = 0; mask < 2 ** supports.length; mask++) {
    const chosen = supports.filter((_, index) => (mask & 2 ** index) !== 0);
    if (chosen.length > capacity || !await supportSetCompatible(group(), chosen.map(gem => input(gem.skill_id)), catalog)) continue;
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
    const oracle = await exhaustiveSupports(catalog, 2, scenario.score);
    const isolatedCompatible = await Promise.all(scenario.supports.map(gem => supportSetCompatible(group(), [input(gem.skill_id)], catalog)));
    const isolated = scenario.supports.filter((_, index) => isolatedCompatible[index])
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

test('player exclusions constrain probes and current-set seeds without changing the equipped baseline', async () => {
  const catalog = [active, gem('Costly'), gem('CostlyII', { family: 'Costly' }), gem('A'), gem('B'), gem('Greedy')];
  const request: CalculateBuildRequest = { character: { level: 80 }, socket_groups: [group(['Costly', 'A']),
    { ...group(['Costly']), weapon_set: 2, enabled: false }] };
  const original = structuredClone(request);
  const score = (ids: string[]) => 100 + (ids.includes('Costly') ? 500 : 0)
    + (ids.includes('CostlyII') ? 10 : 0) + (ids.includes('Greedy') ? 900 : 0)
    + (ids.includes('A') && ids.includes('B') ? 700 : 0);
  const evaluate = evaluator(score);
  const probed: string[][] = [];
  const result = await optimizeSupports({ request, groupIndex: 0, catalog, capacity: 2, objective,
    excludedSkillIds: ['Costly', 'Greedy', 'Costly', 'StaleId'], evaluate: async options => {
      for (const variant of options.variants) {
        if (!variant.socket_groups) continue; // The identity call must keep the real equipped build.
        probed.push(variant.socket_groups[0].gems.slice(1).map(gem => gem.skill_id));
        expect(variant.socket_groups[1]).toEqual(original.socket_groups![1]);
      }
      return evaluate(options);
    } });
  expect(request).toEqual(original);
  expect(result.baseline.TotalDPS).toBe(600);
  expect(result.candidates).toBe(3);
  expect(probed.every(ids => !ids.includes('Costly') && !ids.includes('Greedy'))).toBe(true);
  expect(probed.some(ids => ids.includes('CostlyII'))).toBe(true);
  const oracle = await exhaustiveSupports(catalog.filter(gem => !['Costly', 'Greedy'].includes(gem.skill_id)), 2, score);
  expect(result.plans[0].stats.TotalDPS).toBe(oracle.best);
  expect(result.plans[0].supports.map(gem => gem.skill_id).sort()).toEqual(['A', 'B']);
});

test('excluding an equipped harmful support evaluates its removal, including an empty allowed pool', async () => {
  const catalog = [active, gem('Harmful'), gem('Neutral')];
  const score = (ids: string[]) => 100 - (ids.includes('Harmful') ? 50 : 0);
  for (const equipped of [['Harmful', 'Neutral'], ['Harmful']]) {
    const result = await optimizeSupports({ request: { character: { level: 80 }, socket_groups: [group(equipped)] },
      groupIndex: 0, catalog, capacity: 2, objective, excludedSkillIds: equipped.includes('Neutral')
        ? ['Harmful'] : ['Harmful', 'Neutral'], evaluate: evaluator(score) });
    expect(result.baseline.TotalDPS).toBe(50);
    expect(result.plans.some(plan => plan.supports.length === equipped.length - 1 && plan.stats.TotalDPS === 100)).toBe(true);
    expect(result.plans.every(plan => !plan.supports.some(gem => gem.skill_id === 'Harmful'))).toBe(true);
  }
});

test('excluded type-enablers and disabled lineage gems cannot survive in an equipped search seed', async () => {
  const catalog = [active, gem('Enabler', { add_skill_types: ['Minion'] }), gem('NeedsMinion', { require_skill_types: ['Minion'] }),
    gem('Lineage', { is_lineage: true }), gem('Alternative')];
  const result = await optimizeSupports({ request: { socket_groups: [group(['Enabler', 'NeedsMinion', 'Lineage'])] },
    groupIndex: 0, catalog, capacity: 3, objective, excludedSkillIds: ['Enabler'], includeLineage: false,
    evaluate: evaluator(ids => 100 + (ids.includes('Alternative') ? 100 : 0)) });
  expect(result.plans[0].supports.map(gem => gem.skill_id)).toEqual(['Alternative']);
  expect(result.plans.every(plan => plan.supports.every(gem => gem.skill_id === 'Alternative'))).toBe(true);
});


test('lineage supports respect the default copy limit across skill and weapon groups', () => {
  const lineage = gem('Lineage', { is_lineage: true });
  const groups = [group(), { ...group(['Lineage']), enabled: false, weapon_set: 2 as const }];
  expect(lineageAvailable(lineage, groups, 0)).toBe(false);
  expect(lineageAvailable(lineage, groups, 1)).toBe(true);
  expect(lineageAvailable(lineage, groups, 0, 2)).toBe(true);
  expect(lineageAvailable(gem('Lineage'), groups, 0)).toBe(true);
});


test('candidate screening does not let a different optional support suppress a legal conditional pair', async () => {
  const catalog = [active, gem('AddsMinion', { add_skill_types: ['Minion'] }),
    gem('AddsArea', { add_skill_types: ['Area'] }),
    gem('MinionWithoutArea', { require_skill_types: ['Minion'], exclude_skill_types: ['Area'] })];
  expect(eligibleSupports(group(), catalog, 50).gems.map(gem => gem.skill_id)).toContain('MinionWithoutArea');
  expect(await supportSetCompatible(group(), [input('AddsMinion'), input('MinionWithoutArea')], catalog)).toBe(true);
  expect(await supportSetCompatible(group(), [input('AddsArea'), input('AddsMinion'), input('MinionWithoutArea')], catalog)).toBe(false);
});


test('engine rejection filters candidates before scoring and retains item provenance', async () => {
  engine.judge.mockResolvedValueOnce([false]);
  const itemGroup = { ...group(), source: 'Weapon 1' };
  expect(await supportSetCompatible(itemGroup, [input('New')], [active, gem('New')])).toBe(false);
  expect(engine.judge).toHaveBeenLastCalledWith([{ ...itemGroup, gems: [input('Fireball'), input('New')] }]);
});


test('engine-rejected sets never reach variant scoring', async () => {
  engine.judge.mockResolvedValueOnce([false]);
  const evaluate = vi.fn(evaluator(() => 100));
  const result = await optimizeSupports({ request: { socket_groups: [group()] }, groupIndex: 0,
    catalog: [active, gem('New')], capacity: 1, objective, evaluate });
  expect(result.evaluated).toBe(0);
  expect(result.plans).toEqual([]);
  expect(evaluate).toHaveBeenCalledTimes(1); // Only the unchanged baseline.
});

test('cancelling compatibility rejects the late batch and never starts the next batch', async () => {
  const controller = new AbortController();
  let release!: (rows: boolean[]) => void;
  engine.judge.mockImplementationOnce(async () => new Promise<boolean[]>(resolve => { release = resolve; }));
  const before = engine.judge.mock.calls.length;
  const pending = supportSetsCompatible(group(), Array.from({ length: 513 }, () => [input('New')]),
    [active, gem('New')], controller.signal);
  const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
  await vi.waitFor(() => expect(engine.judge.mock.calls.length).toBe(before + 1));
  controller.abort();
  release(Array(512).fill(true));
  await rejected;
  expect(engine.judge.mock.calls.length).toBe(before + 1);
});

// Explicitly synthetic raw engine diagnostics; these are not claims about real
// game-effect coverage. Reversed rows model backend index-based association.
function diagnosticEvaluator(baselineUnsupported: string[], diagnose: (supports: GemInput[]) => {
  stats: Record<string, number>; unsupported: string[]; error?: string;
}) {
  return async ({ request, variants }: EvaluateOptions) => ({
    baseline: { TotalDPS: 100, TotalEHP: 1000 },
    aborted: false,
    results: variants.map((variant, index) => {
      const identity = !variant.socket_groups;
      const supports = (variant.socket_groups ?? request.socket_groups)![0].gems.slice(1);
      const row = identity
        ? { stats: { TotalDPS: 100, TotalEHP: 1000 }, unsupported: baselineUnsupported, error: undefined }
        : diagnose(supports);
      return { index, label: null, error: row.error ?? null, stats: row.stats, unsupported: row.unsupported };
    }).reverse(),
  });
}

test('raw baseline reasons retain order and duplicates while reversed results attach only new reasons to their exact complete set', async () => {
  const shared = ['Synthetic baseline Z', 'Synthetic baseline A', 'Synthetic baseline Z'];
  const request = { character: { level: 80 }, socket_groups: [group()] };
  const result = await optimizeSupports({ request, groupIndex: 0, catalog: [active, gem('BadA'), gem('BadB'), gem('Good')],
    capacity: 1, objective, evaluate: diagnosticEvaluator(shared, supports => {
      const id = supports[0]?.skill_id;
      return { stats: { TotalDPS: id?.startsWith('Bad') ? 900 : 200, TotalEHP: 1000 },
        unsupported: id?.startsWith('Bad')
          ? [shared[0], `Synthetic ${id} first`, shared[1], `Synthetic ${id} second`, `Synthetic ${id} first`]
          : [shared[1], shared[0]] };
    }) });
  expect(result.baselineUnsupported).toEqual(shared);
  expect(result.unmodeled).toBe(2);
  expect(result.excluded).toHaveLength(2);
  for (const id of ['BadA', 'BadB']) {
    const edit = result.excluded.find(edit => edit.supports[0]?.skill_id === id)!;
    expect(edit.supports).toEqual([input(id)]);
    expect(edit.variant).toEqual({ socket_groups: [group([id])] });
    expect(edit.unsupported).toEqual([`Synthetic ${id} first`, `Synthetic ${id} second`, `Synthetic ${id} first`]);
    expect(edit.error).toBeUndefined();
  }
  expect(result.plans.map(plan => plan.supports)).toEqual([[input('Good')]]);
});

test('combination-only diagnostics reject the complete pair but keep both individually modeled improvements', async () => {
  const result = await optimizeSupports({ request: { character: { level: 80 }, socket_groups: [group()] },
    groupIndex: 0, catalog: [active, gem('A'), gem('B')], capacity: 2, objective,
    evaluate: diagnosticEvaluator(['Synthetic shared Guard'], supports => ({
      stats: { TotalDPS: supports.length === 2 ? 1000 : supports.length ? 200 : 100, TotalEHP: 1000 },
      unsupported: ['Synthetic shared Guard', ...(supports.length === 2 ? ['Synthetic pair-only limit'] : [])],
    })) });
  expect(result.excluded).toHaveLength(1);
  expect(result.excluded[0].supports.map(gem => gem.skill_id).sort()).toEqual(['A', 'B']);
  expect(result.excluded[0].variant.socket_groups![0].gems.slice(1)).toEqual(result.excluded[0].supports);
  expect(result.excluded[0].unsupported).toEqual(['Synthetic pair-only limit']);
  expect(result.unmodeled).toBe(1);
  expect(result.plans.map(plan => plan.supports[0].skill_id).sort()).toEqual(['A', 'B']);
  expect(result.plans.every(plan => plan.supports.length === 1)).toBe(true);
});

test('errors and nonfinite stats retain diagnostics without changing unmodeled precedence or entering recommendations', async () => {
  const shared = 'Synthetic shared Guard';
  const outcomes = [
    { id: 'Error', stats: { TotalDPS: 1000, TotalEHP: 1000 }, error: 'Synthetic engine failure: exact raw text',
      unsupported: [shared, 'Synthetic error limit', 'Synthetic error limit'] },
    { id: 'Nonfinite', stats: { TotalDPS: Infinity, TotalEHP: 1000 }, unsupported: [shared, 'Synthetic nonfinite limit'] },
    { id: 'NonfiniteOnly', stats: { TotalDPS: 1000, TotalEHP: NaN }, unsupported: [shared] },
    { id: 'FiniteLimit', stats: { TotalDPS: 1000, TotalEHP: 1000 }, unsupported: [shared, 'Synthetic finite limit'] },
    { id: 'Good', stats: { TotalDPS: 200, TotalEHP: 1000 }, unsupported: [shared] },
  ];
  const result = await optimizeSupports({ request: { character: { level: 80 }, socket_groups: [group()] },
    groupIndex: 0, catalog: [active, ...outcomes.map(row => gem(row.id))], capacity: 1, objective,
    evaluate: diagnosticEvaluator([shared], supports => outcomes.find(row => row.id === supports[0]?.skill_id)
      ?? { stats: { TotalDPS: 100, TotalEHP: 1000 }, unsupported: [shared] }) });
  expect(result.unmodeled).toBe(1);
  expect(result.excluded).toHaveLength(4);
  for (const outcome of outcomes.filter(row => row.id !== 'Good')) {
    const edit = result.excluded.find(edit => edit.supports[0]?.skill_id === outcome.id)!;
    expect(edit.supports).toEqual([input(outcome.id)]);
    expect(edit.variant.socket_groups).toEqual([group([outcome.id])]);
    expect(edit.unsupported).toEqual(outcome.unsupported.filter(reason => reason !== shared));
    expect(edit.error).toBe('error' in outcome ? outcome.error : undefined);
    expect(edit.invalidStats).toBe(Object.values(outcome.stats).some(value => !Number.isFinite(value)) ? true : undefined);
  }
  expect(result.plans.map(plan => plan.supports)).toEqual([[input('Good')]]);
});

test('rejected removals and replacements record full evaluated sets with equipped metadata and other-group provenance', async () => {
  const old = { ...input('Old'), level: 3, quality: 17, stat_set_index: 2 };
  const keep = { ...input('Keep'), level: 2, quality: 13, stat_set_index: 3 };
  const other = { ...group(['Other']), enabled: false, weapon_set: 2 as const };
  const target = { ...group(), gems: [input('Fireball'), old, keep], source: 'Weapon 1', weapon_set: 1 as const };
  const request = { character: { level: 80 }, socket_groups: [target, other] };
  const original = structuredClone(request);
  const result = await optimizeSupports({ request, groupIndex: 0,
    catalog: [active, gem('Old'), gem('Keep'), gem('New'), gem('Other')], capacity: 2, objective,
    excludedSkillIds: ['Other'], evaluate: diagnosticEvaluator(['Synthetic shared Guard'], supports => {
      const ids = supports.map(gem => gem.skill_id);
      const rejected = ids.length === 1 && ids[0] === 'Keep' || ids.includes('New');
      return { stats: { TotalDPS: rejected ? 1000 : 100, TotalEHP: 1000 },
        unsupported: ['Synthetic shared Guard', ...(rejected ? [`Synthetic set limit: ${ids.join('+')}`] : [])] };
    }) });
  const removal = result.excluded.find(edit => edit.supports.length === 1 && edit.supports[0].skill_id === 'Keep')!;
  expect(removal.supports).toEqual([keep]);
  expect(removal.variant.socket_groups).toEqual([{ ...target, gems: [input('Fireball'), keep] }, other]);
  expect(removal.unsupported).toEqual(['Synthetic set limit: Keep']);
  const replacement = result.excluded.find(edit => edit.supports.some(gem => gem.skill_id === 'New')
    && edit.supports.some(gem => gem.skill_id === 'Old'))!;
  expect(replacement.supports).toEqual([old, input('New')]);
  expect(replacement.variant.socket_groups).toEqual([{ ...target, gems: [input('Fireball'), old, input('New')] }, other]);
  expect(replacement.unsupported).toEqual(['Synthetic set limit: Old+New']);
  expect(result.plans).toEqual([]);
  expect(request).toEqual(original);
});
