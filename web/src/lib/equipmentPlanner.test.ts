import { expect, test, vi } from 'vitest';
import type { CalculateBuildRequest, ItemAugmentInfo, RuneCatalogEntry } from '../api/types';
import { EQUIPMENT_CANDIDATE_LIMIT, planEquipmentUpgrades, type EquipmentCandidate, type EquipmentPlanningDependencies } from './equipmentPlanner';
import type { EvaluateOptions, EvaluateResult } from './optimize';
import type { TradeCatalog } from './tradeOptimizer';

const ring = { name: 'Sapphire Ring', category: 'accessory.ring', domain: 'item', tags: [], level: 1, implicits: [] };
const helmet = { name: 'Iron Hat', category: 'armour.helmet', domain: 'item', tags: [], level: 1, implicits: [] };
const catalog: TradeCatalog = { bases: [ring, helmet], mods: [] };
const request: CalculateBuildRequest = { character: { level: 90 }, items: [
  { slot: 'ring1', text: `Rarity: RARE\nOld One\n${ring.name}` },
  { slot: 'ring2', text: `Rarity: RARE\nOld Two\n${ring.name}` },
  { slot: 'helmet', text: `Rarity: RARE\nOld Hat\n${helmet.name}` },
] };
const candidate = (id: string, slot: string, base = ring.name, itemId = id, effect = ''): EquipmentCandidate => ({
  id, itemId, label: id, slot, text: `Rarity: RARE\n${id}\n${base}${effect ? `\n${effect}` : ''}`,
});
const rune = (name: string, limit?: number): RuneCatalogEntry => ({ name, name_zh_cn: null, name_zh_tw: null,
  kind: 'Rune', is_soul_core: false, lines: [], ...(limit === undefined ? {} : { limit }) });
const limited = rune('Limited Rune', 1);
const augmentInfo = async (text: string): Promise<ItemAugmentInfo> => ({
  sockets: text.includes('Rune: Limited Rune') ? 1 : 0,
  max_sockets: 2,
  runes: text.includes('Rune: Limited Rune') ? ['Limited Rune'] : [],
  editable: true,
  options: [limited],
});
const evaluate = vi.fn(async ({ request: base, variants }: EvaluateOptions): Promise<EvaluateResult> => ({
  baseline: { TotalDPS: 100, TotalEHP: 1000 }, aborted: false,
  results: variants.map((variant, index) => {
    const items = variant.set_items ?? [];
    const ids = items.map(item => item.text.split('\n')[1]);
    const gain = ids.includes('a') && ids.includes('b') ? 30 : ids.includes('a') ? -10 : ids.includes('b') ? -5 : 0;
    expect(base.items?.find(item => item.slot === 'helmet')?.text).toContain('Old Hat');
    return { index, label: null, error: null, stats: { TotalDPS: 100 + gain, TotalEHP: 1000 }, unsupported: [] };
  }),
}));
const dependencies = (overrides: Partial<EquipmentPlanningDependencies> = {}): EquipmentPlanningDependencies => ({
  catalog, translate: async lines => lines, augmentInfo, runeCatalog: async () => [limited], evaluate, ...overrides,
});

test('a beneficial pair survives individually harmful singles and retains unrelated equipment', async () => {
  evaluate.mockClear();
  const before = structuredClone(request);
  const result = await planEquipmentUpgrades({ request, candidates: [candidate('a', 'ring1'), candidate('b', 'ring2')] }, dependencies());
  expect(result.plans.map(plan => plan.candidateIds)).toEqual([[], ['a'], ['b'], ['a', 'b']]);
  expect(result.plans.map(plan => plan.stats.TotalDPS)).toEqual([100, 90, 95, 130]);
  expect(result.plans[3].items.map(item => item.slot)).toEqual(['ring1', 'ring2']);
  expect(result.evaluated).toBe(4);
  expect(request).toEqual(before);
});

test('a beneficial triple is evaluated as a complete loadout despite losing singles and pairs', async () => {
  const choices = [candidate('a', 'ring1'), candidate('b', 'ring2'), candidate('c', 'helmet', helmet.name)];
  const result = await planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 3 }, dependencies({
    evaluate: async ({ variants }) => ({ baseline: { TotalDPS: 100 }, aborted: false,
      results: variants.map((variant, index) => {
        const count = variant.set_items?.length ?? 0;
        return { index, label: null, error: null, stats: { TotalDPS: count === 3 ? 150 : 100 - 10 * count }, unsupported: [] };
      }) }),
  }));
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'a,b,c')?.stats.TotalDPS).toBe(150);
  expect(result.evaluated).toBe(8);
  expect(result.limited).toBe(false);
  expect(result.plans.filter(plan => plan.candidateIds.length === 2).every(plan => plan.stats.TotalDPS === 80)).toBe(true);
});

test('maximum one replacement omits pairs and rejects invalid maximums', async () => {
  const choices = [candidate('a', 'ring1'), candidate('b', 'ring2')];
  const result = await planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 1 }, dependencies());
  expect(result.plans.map(plan => plan.candidateIds)).toEqual([[], ['a'], ['b']]);
  expect(result.evaluated).toBe(3);
  await expect(planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 11 }, dependencies()))
    .rejects.toThrow('Maximum equipment replacements');
});

test('three-piece plans never reuse a pasted item across destinations', async () => {
  const choices = [candidate('a', 'ring1'), candidate('b', 'ring2'),
    candidate('c', 'helmet', helmet.name, 'a'), candidate('d', 'helmet', helmet.name)];
  const result = await planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 3 }, dependencies());
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'a,b,c')).toBeUndefined();
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'a,b,d')).toBeDefined();
});

test('bounded search retains every legal single and pair and reports its limit', async () => {
  const glove = { ...ring, name: 'Iron Gloves', category: 'armour.gloves' };
  const slots = ['ring1', 'ring2', 'helmet', 'gloves'];
  const bases = [ring.name, ring.name, helmet.name, glove.name];
  const choices = slots.flatMap((slot, slotIndex) => Array.from({ length: 4 }, (_, index) =>
    candidate(`${slot}-${index}`, slot, bases[slotIndex])));
  const result = await planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 4 },
    dependencies({ catalog: { bases: [ring, helmet, glove], mods: [] }, evaluate: async ({ variants }) => ({
      baseline: { TotalDPS: 100 }, aborted: false,
      results: variants.map((variant, index) => ({ index, label: null, error: null,
        stats: { TotalDPS: 100 + (variant.set_items?.length ?? 0) }, unsupported: [] })),
    }) }));
  expect(result.limited).toBe(true);
  expect(result.evaluated).toBeLessThanOrEqual(512);
  expect(result.plans.filter(plan => plan.candidateIds.length === 1)).toHaveLength(16);
  expect(result.plans.filter(plan => plan.candidateIds.length === 2)).toHaveLength(96);
  expect(result.plans.some(plan => plan.candidateIds.length === 3)).toBe(true);
});

test('a limited ten-piece search reserves evaluations for its deepest reachable combinations', async () => {
  const definitions = [
    ['weapon1', 'Practice Sword', 'weapon.onesword'], ['weapon2', 'Practice Shield', 'armour.shield'],
    ['helmet', 'Iron Hat', 'armour.helmet'], ['bodyarmour', 'Iron Coat', 'armour.chest'],
    ['gloves', 'Iron Gloves', 'armour.gloves'], ['boots', 'Iron Boots', 'armour.boots'],
    ['amulet', 'Iron Amulet', 'accessory.amulet'], ['ring1', 'Sapphire Ring', 'accessory.ring'],
    ['ring2', 'Sapphire Ring', 'accessory.ring'], ['belt', 'Iron Belt', 'accessory.belt'],
  ] as const;
  const bases = definitions.map(([, name, category]) => ({ ...ring, name, category }));
  const choices = definitions.flatMap(([slot, name], index) => [
    candidate(`${slot}-first`, slot, name),
    ...(index < 6 ? [candidate(`${slot}-second`, slot, name)] : []),
  ]);
  const result = await planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 10 },
    dependencies({ catalog: { bases, mods: [] }, evaluate: async ({ variants }) => ({
      baseline: { TotalDPS: 100 }, aborted: false,
      results: variants.map((variant, index) => ({ index, label: null, error: null,
        stats: { TotalDPS: 100 + (variant.set_items?.length ?? 0) }, unsupported: [] })),
    }) }));
  expect(result.limited).toBe(true);
  expect(result.evaluated).toBeLessThanOrEqual(512);
  expect(Math.max(...result.plans.filter(plan => !plan.error).map(plan => plan.candidateIds.length))).toBe(10);
});

test('two different rings can exchange destinations, but one pasted item cannot occupy both', async () => {
  const choices = [candidate('left-to-right', 'ring2', ring.name, 'left'),
    candidate('right-to-left', 'ring1', ring.name, 'right'),
    candidate('same-left', 'ring1', ring.name, 'left')];
  const result = await planEquipmentUpgrades({ request, candidates: choices }, dependencies());
  expect(result.plans.map(plan => plan.candidateIds)).toContainEqual(['left-to-right', 'right-to-left']);
  expect(result.plans.map(plan => plan.candidateIds)).not.toContainEqual(['left-to-right', 'same-left']);
  expect(result.plans.map(plan => plan.candidateIds)).not.toContainEqual(['right-to-left', 'same-left']);
});

test('final equipment catches a rune conflict across two replacements and retains unaffected equipment', async () => {
  const choices = [candidate('a', 'ring1', ring.name, 'one', 'Rune: Limited Rune'),
    candidate('b', 'ring2', ring.name, 'two', 'Rune: Limited Rune')];
  const result = await planEquipmentUpgrades({ request, candidates: choices }, dependencies());
  expect(result.plans.find(plan => plan.candidateIds.length === 2)).toMatchObject({ error: 'augment-limit', stats: {} });
  expect(result.plans.filter(plan => plan.candidateIds.length === 1).every(plan => !plan.error)).toBe(true);
  expect(result.evaluated).toBe(3);
  const occupied = { ...request, items: [...request.items!, { slot: 'amulet', text: 'Rune: Limited Rune' }] };
  const single = await planEquipmentUpgrades({ request: occupied, candidates: [choices[0]] }, dependencies());
  expect(single.plans[1].error).toBe('augment-limit');
  expect(single.evaluated).toBe(1);
});

test('a pair can release an occupied rune limit that blocks the second item alone', async () => {
  const occupied: CalculateBuildRequest = { ...request, items: [
    { slot: 'ring1', text: `Rarity: RARE\nOld Limited\n${ring.name}\nRune: Limited Rune` },
    ...request.items!.filter(item => item.slot !== 'ring1'),
  ] };
  const choices = [candidate('a', 'ring1'), candidate('b', 'ring2', ring.name, 'other', 'Rune: Limited Rune')];
  const result = await planEquipmentUpgrades({ request: occupied, candidates: choices }, dependencies());
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'b')?.error).toBe('augment-limit');
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'a,b')?.error).toBeUndefined();
  expect(result.plans.find(plan => plan.candidateIds.join(',') === 'a,b')?.stats.TotalDPS).toBe(130);
  expect(result.evaluated).toBe(3);
});

test('invalid candidates and backend errors remain visible without hiding valid plans', async () => {
  const result = await planEquipmentUpgrades({ request, candidates: [candidate('wrong', 'Flask 1'), candidate('a', 'ring1')],
  }, dependencies({ evaluate: async options => ({ baseline: { TotalDPS: 100 }, aborted: false,
    results: options.variants.map((_, index) => ({ index, label: null, stats: index ? {} as Record<string, number> : { TotalDPS: 100 },
      error: index ? 'calc-failed' : null, unsupported: [] })) }) }));
  expect(result.plans[0].stats.TotalDPS).toBe(100);
  expect(result.plans.find(plan => plan.candidateIds[0] === 'wrong')?.error).toBe('wrong-slot');
  expect(result.plans.find(plan => plan.candidateIds[0] === 'a')?.error).toBe('calc-failed');
  expect(result.evaluated).toBe(2);
});

test('a new bow in an empty main hand must still fit an unchanged offhand', async () => {
  const bow = { ...ring, name: 'Practice Bow', category: 'weapon.bow' };
  const shield = { ...ring, name: 'Practice Shield', category: 'armour.shield' };
  const build: CalculateBuildRequest = { character: { level: 90 }, items: [
    { slot: 'weapon2', text: `Rarity: RARE\nOld Shield\n${shield.name}` },
  ] };
  const result = await planEquipmentUpgrades({ request: build, candidates: [candidate('bow', 'weapon1', bow.name)] },
    dependencies({ catalog: { bases: [bow, shield], mods: [] }, evaluate: async options => ({
      baseline: { TotalDPS: 100 }, aborted: false,
      results: options.variants.map((_, index) => ({ index, label: null, error: null, stats: { TotalDPS: 100 }, unsupported: [] })),
    }) }));
  expect(result.plans[1].error).toBe('weapon-type');
  expect(result.evaluated).toBe(1);
});

test('unknown augments and unsupported effects cannot become silently applicable', async () => {
  const result = await planEquipmentUpgrades({ request, candidates: [candidate('a', 'ring1', ring.name, 'a', 'Rune: Mystery')],
  }, dependencies({ augmentInfo: async () => ({ sockets: 1, max_sockets: 1, runes: ['Mystery'], editable: false,
    reason: 'unknown_augments', options: [] }), evaluate: async options => ({ baseline: { TotalDPS: 100 }, aborted: false,
    results: options.variants.map((_, index) => ({ index, label: null, error: null,
      stats: { TotalDPS: 100 + index }, unsupported: index ? ['Unknown effect'] : [] })) }) }));
  expect(result.plans[1]).toMatchObject({ error: 'unknown-equipped-augments', warnings: ['unknown-equipped-augments'],
    unsupported: ['Unknown effect'], stats: { TotalDPS: 101 } });
});

test('sixteen candidates stay under the variant cap and an aborted run publishes no plans', async () => {
  const choices = Array.from({ length: EQUIPMENT_CANDIDATE_LIMIT }, (_, index) =>
    candidate(`item-${index}`, index % 2 ? 'ring1' : 'ring2'));
  const result = await planEquipmentUpgrades({ request, candidates: choices }, dependencies());
  expect(result.evaluated).toBe(81);
  expect(result.evaluated).toBeLessThan(512);
  const controller = new AbortController();
  controller.abort();
  const stopped = vi.fn();
  await expect(planEquipmentUpgrades({ request, candidates: choices, signal: controller.signal },
    dependencies({ evaluate: stopped }))).rejects.toThrow();
  expect(stopped).not.toHaveBeenCalled();
});

test('cancelled evaluations and non-finite values never enter a published ranking', async () => {
  const choices = [candidate('a', 'ring1')];
  const rows = ({ variants }: EvaluateOptions) => variants.map((_, index) => ({
    index, label: null, error: null, stats: { TotalDPS: 100 + index }, unsupported: [],
  }));
  await expect(planEquipmentUpgrades({ request, candidates: choices }, dependencies({
    evaluate: async options => ({ baseline: { TotalDPS: 100 }, results: rows(options), aborted: true }),
  }))).rejects.toThrow('cancelled');

  const controller = new AbortController();
  await expect(planEquipmentUpgrades({ request, candidates: choices, signal: controller.signal }, dependencies({
    evaluate: async options => { controller.abort(); return { baseline: { TotalDPS: 100 }, results: rows(options), aborted: false }; },
  }))).rejects.toThrow('cancelled');

  await expect(planEquipmentUpgrades({ request, candidates: choices }, dependencies({
    evaluate: async options => ({ baseline: { TotalDPS: Infinity }, results: rows(options), aborted: false }),
  }))).rejects.toThrow('non-finite-baseline');

  const result = await planEquipmentUpgrades({ request, candidates: choices }, dependencies({
    evaluate: async options => ({ baseline: { TotalDPS: 100 }, aborted: false,
      results: rows(options).map(row => row.index ? { ...row, stats: { TotalDPS: Number.NaN } } : row) }),
  }));
  expect(result.plans[1]).toMatchObject({ error: 'non-finite-stats', stats: {} });
});

test('cancellation after the first phase stops multi-item expansion', async () => {
  const controller = new AbortController();
  let calls = 0;
  const choices = [candidate('a', 'ring1'), candidate('b', 'ring2'), candidate('c', 'helmet', helmet.name)];
  await expect(planEquipmentUpgrades({ request, candidates: choices, maxReplacements: 3, signal: controller.signal },
    dependencies({ evaluate: async ({ variants }) => {
      calls += 1;
      controller.abort();
      return { baseline: { TotalDPS: 100 }, aborted: false,
        results: variants.map((_, index) => ({ index, label: null, error: null,
          stats: { TotalDPS: 100 }, unsupported: [] })) };
    } }))).rejects.toThrow('cancelled');
  expect(calls).toBe(1);
});
