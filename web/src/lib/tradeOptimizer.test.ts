import { describe, expect, test } from 'vitest';
import { affixPool, basesForSlot, categoryAffixPool, referenceBase, combinationLegal, tradeItemVariant, optimizeTradeAffixes, type TradeAffix, type TradeBase } from './tradeOptimizer';
import type { EvaluateOptions, EvaluateResult } from './optimize';

const base: TradeBase = { name: 'Broadhead Quiver', category: 'armour.quiver', tags: ['quiver', 'default'], level: 1, implicits: [] };
function affix(id: string, kind: TradeAffix['kind'] = 'prefix', group = id): TradeAffix {
  return { id, group, kind, level: 1, lines: [`10 ${id}`], weights: [['quiver', 1], ['default', 0]],
    stats: [{ id: `explicit.${id}`, line: `10 ${id}`, value: 10 }] };
}
const objective = { stat: 'TotalDPS', constraints: [] };
const evaluator = (score: (text: string) => number, visited: string[] = []) => async ({ variants }: EvaluateOptions): Promise<EvaluateResult> => ({
  baseline: { TotalDPS: 100 }, aborted: false,
  results: variants.map((variant, index) => {
    const text = variant.set_items![0].text;
    visited.push(text);
    return { index, label: null, error: null, stats: { TotalDPS: score(text) } };
  }),
});

describe('legal affix pool', () => {
  test('honors first matching exclusions, item level, and strongest tier', () => {
    const weak = affix('weak');
    const strong = { ...weak, id: 'strong', level: 80, lines: ['20 weak'] };
    const forbidden = { ...affix('blocked'), weights: [['quiver', 0], ['default', 1]] as [string, number][] };
    const catalog = { bases: [base], mods: [weak, strong, forbidden] };
    expect(affixPool(catalog, base, 79).map(mod => mod.id)).toEqual(['weak']);
    expect(affixPool(catalog, base, 80).map(mod => mod.id)).toEqual(['strong']);
  });
  test('excludes wrong slots and mutually exclusive/over-cap affixes', () => {
    const catalog = { bases: [base, { ...base, name: 'Bow', category: 'weapon.bow' }], mods: [] };
    expect(basesForSlot(catalog, 'weapon2')).toEqual([base]);
    expect(basesForSlot(catalog, 'weapon1')[0].category).toBe('weapon.bow');
    expect(combinationLegal([affix('a'), affix('b', 'suffix', 'a')])).toBe(false);
    expect(combinationLegal(['a', 'b', 'c', 'd'].map(id => affix(id)))).toBe(false);
  });
});

test('discovers new affixes and jointly beneficial pairs, not only current item lines', async () => {
  const pool = [affix('flat'), affix('speed', 'suffix'), affix('crit', 'suffix')];
  const result = await optimizeTradeAffixes({ request: { character: { level: 90, class_name: 'Ranger' } },
    slot: 'weapon2', base, pool, itemLevel: 82, objective,
    evaluate: evaluator(text => 100 + (text.includes('flat') ? 10 : 0) +
      (text.includes('speed') && text.includes('crit') ? 100 : 0)),
  });
  expect(result.combinations[0].score).toBe(210);
  expect(result.combinations[0].mods.map(mod => mod.id).sort()).toEqual(['crit', 'flat', 'speed']);
  expect(result.weighted.some(stat => stat.id === 'explicit.flat')).toBe(true);
  expect(result.limited).toBe(false);
});

test('evaluation budget reserves deeper combinations and reports pruning', async () => {
  const pool = Array.from({ length: 16 }, (_, i) => affix(`m${i}`, i < 8 ? 'prefix' : 'suffix'));
  const seen: string[] = [];
  const result = await optimizeTradeAffixes({ request: {}, slot: 'weapon2', base, pool,
    itemLevel: 82, objective, maxEvaluations: 160, beamWidth: 4,
    evaluate: evaluator(text => (text.match(/10 m/g) ?? []).length, seen),
  });
  expect(result.evaluated).toBeLessThanOrEqual(160);
  expect(result.evaluated).toBe(seen.length);
  expect(result.limited).toBe(true);
  expect(result.combinations[0].mods).toHaveLength(6);
  expect(result.combinations.every(combo => combinationLegal(combo.mods))).toBe(true);
});

test('cancellation does not publish incomplete weights as a completed search', async () => {
  const controller = new AbortController();
  controller.abort();
  await expect(optimizeTradeAffixes({ request: {}, slot: 'weapon2', base, pool: [affix('a')],
    itemLevel: 82, objective, signal: controller.signal, evaluate: evaluator(() => 0),
  })).rejects.toThrow();
});


test('jewels enforce two affixes of each kind and preserve other sockets', async () => {
  const jewel = { ...base, category: 'jewel', domain: 'jewel', affix_limit: 2 };
  const pool = ['a', 'b', 'c'].map(id => ({ ...affix(id), domain: 'jewel' }));
  expect(combinationLegal(pool, 2)).toBe(false);
  expect(affixPool({ bases: [jewel], mods: [...pool, affix('equipment-only')] }, jewel, 82)).toHaveLength(3);
  const request = { jewels: [{ socket_node: 10, text: 'old' }, { socket_node: 20, text: 'keep' }] };
  expect(tradeItemVariant(request, 'Jewel@10', 'new').jewels).toEqual([
    { socket_node: 20, text: 'keep' }, { socket_node: 10, text: 'new' },
  ]);
  expect(tradeItemVariant({ flasks: [{ slot: 'Flask 2', text: 'keep' }] }, 'Flask 1', 'new').flasks).toHaveLength(2);
});

test('a cancellation arriving after the last probe still invalidates score-only results', async () => {
  const controller = new AbortController();
  await expect(optimizeTradeAffixes({ request: {}, slot: 'weapon2', base, pool: [affix('a')],
    itemLevel: 82, objective, signal: controller.signal, combinations: false,
    evaluate: async options => {
      const result = await evaluator(() => 100)(options);
      controller.abort();
      return result;
    },
  })).rejects.toThrow();
});

test('automatically detects equipped bases but pools affixes across the whole category', () => {
  const other = { ...base, name: 'Primed Quiver', tags: ['primed', 'quiver', 'default'] };
  const special = { ...affix('other-base'), weights: [['primed', 1], ['default', 0]] as [string, number][] };
  const catalog = { bases: [base, other], mods: [affix('common'), special] };
  expect(referenceBase(catalog, 'weapon2', `Rarity: RARE\nEquipped\n${other.name}`)).toBe(other);
  expect(categoryAffixPool(catalog, 'armour.quiver').map(mod => mod.id)).toEqual(['common', 'other-base']);
  expect(referenceBase(catalog, 'helmet')).toBeUndefined();
});

test('weights include interactions with the equipped item without discarding saturated affixes', async () => {
  const current = `Rarity: RARE\nEquipped\n${base.name}\n10 flat`;
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon2', text: current }] },
    slot: 'weapon2', base, pool: [affix('speed', 'suffix'), affix('crit', 'suffix')], itemLevel: 82, objective, combinations: false,
    evaluate: evaluator(text => 100 + (text.includes('speed') ? text.includes('flat') ? 40 : 10 : 0)
      + (text.includes('crit') && !text.includes('Equipped') ? 20 : 0)),
  });
  expect(result.weighted.find(row => row.id === 'explicit.speed')?.gain).toBe(25);
  expect(result.weighted.find(row => row.id === 'explicit.crit')?.gain).toBe(10);
  expect(result.weighted[0].gainPercent).toBe(25);
  expect(result.evaluated).toBe(6);
});

test('combination references only use base-compatible affixes and obey survival constraints', async () => {
  const allowed = affix('allowed'); const impossible = affix('impossible');
  const result = await optimizeTradeAffixes({ request: { character: { level: 90, class_name: 'Ranger' } }, slot: 'weapon2', base,
    pool: [allowed, impossible], combinationPool: [allowed], itemLevel: 82,
    objective: { stat: 'TotalDPS', constraints: [{ stat: 'TotalDPS', min: 101 }] },
    evaluate: evaluator(text => 100 + (text.includes('allowed') ? 5 : 0) + (text.includes('impossible') ? 100 : 0)) });
  expect(result.combinations.length).toBeGreaterThan(0);
  expect(result.combinations.every(entry => !entry.text.includes('impossible') && entry.score >= 101)).toBe(true);
});

test('reference equipment and category affixes respect character level without changing category', () => {
  const high = { ...base, name: 'Endgame Quiver', level: 80, tags: ['high', 'default'] };
  const highMod = { ...affix('high'), weights: [['high', 1], ['default', 0]] as [string, number][] };
  const catalog = { bases: [high, base], mods: [highMod, affix('low')] };
  expect(referenceBase(catalog, 'weapon2', 'Endgame Quiver', undefined, 20)).toEqual(base);
  expect(categoryAffixPool(catalog, 'armour.quiver', 100, 20).map(mod => mod.id)).toEqual(['low']);
});
