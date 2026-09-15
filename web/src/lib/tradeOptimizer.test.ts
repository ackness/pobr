import { describe, expect, test } from 'vitest';
import { affixPool, basesForSlot, categoryAffixPool, categorySearchMods, referenceBase, combinationLegal, tradeItemVariant, optimizeTradeAffixes, type TradeAffix, type TradeBase, type TradeSearchMod } from './tradeOptimizer';
import type { EvaluateOptions, EvaluateResult } from './optimize';
import { scoreEquipment } from './equipmentScore';
import { buildTradeQuery, type WeightedStat } from './trade';

const base: TradeBase = { name: 'Broadhead Quiver', category: 'armour.quiver', tags: ['quiver', 'default'], level: 1, implicits: [] };
function affix(id: string, kind: TradeAffix['kind'] = 'prefix', group = id): TradeAffix {
  return { id, group, kind, level: 1, lines: [`10 ${id}`], weights: [['quiver', 1], ['default', 0]],
    stats: [{ id: `explicit.${id}`, line: `10 ${id}`, value: 10 }] };
}
const objective = { stat: 'TotalDPS', constraints: [] };
const special: TradeSearchMod = { id: 'alloy:hybrid', source: 'alloy', categories: [base.category], level: 65,
  lines: ['10 flat', '10 speed'], stats: [affix('flat').stats[0], affix('speed').stats[0]] };
const evaluator = (score: (text: string) => number, visited: string[] = []) => async ({ variants }: EvaluateOptions): Promise<EvaluateResult> => ({
  baseline: { TotalDPS: 100 }, aborted: false,
  results: variants.map((variant, index) => {
    const text = variant.set_items![0].text;
    visited.push(text);
    return { index, label: null, error: null, stats: { TotalDPS: score(text) } };
  }),
});

describe('legal affix pool', () => {
  test('special crafting sources respect categories and levels without inventing ordinary crafting recipes', () => {
    const catalog = { bases: [base], mods: [affix('ordinary')], search_mods: [special] };
    expect(categorySearchMods(catalog, base.category, 64)).toEqual([]);
    expect(categorySearchMods(catalog, 'accessory.ring')).toEqual([]);
    expect(categorySearchMods(catalog, base.category, 65)).toEqual([special]);
    expect(affixPool(catalog, base, 100).map(mod => mod.id)).toEqual(['ordinary']);
    expect(categorySearchMods({ bases: [], mods: [] }, base.category)).toEqual([]);
  });
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

test('special hybrid components get independent, deduplicated query weights and the actual crafted current minimum', async () => {
  const current = `Rarity: RARE\nEquipped\n${base.name}\n{crafted}5 flat\n{crafted}8 speed`;
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon2', text: current }] },
    slot: 'weapon2', base, pool: [affix('flat')], searchMods: [special], itemLevel: 82, objective,
    evaluate: evaluator(text => 100 + Number(text.match(/(\d+) flat/)?.[1] ?? 0) + Number(text.match(/(\d+) speed/)?.[1] ?? 0) * 2),
  });
  expect(result.weighted).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: 'explicit.flat', weight: 10, gain: 10 }),
    expect.objectContaining({ id: 'explicit.speed', weight: 20, gain: 20 }),
  ]));
  expect(result.weighted).toHaveLength(2);
  expect(result.currentItemScore).toMatchObject({ complete: true, score: 210 });
  expect(result.minimumWeight).toBe(210);
  expect(result.combinations.every(combo => combo.mods.every(mod => mod.id === 'flat'))).toBe(true);
});

test('partially unmodeled compound probes do not publish inflated weights in score-only mode', async () => {
  const unsafe = { ...special, stats: [{ id: 'explicit.compound', line: '1000 boost Unmodeled drawback', value: 1000 }] };
  const result = await optimizeTradeAffixes({ request: {}, slot: 'weapon2', base, pool: [affix('safe')],
    searchMods: [unsafe], itemLevel: 82, objective, combinations: false,
    evaluate: async options => {
      const result = await evaluator(text => text.includes('boost') ? 1100 : text.includes('safe') ? 120 : 100)(options);
      return { ...result, results: result.results.map(row => ({ ...row,
        unsupported: options.variants[row.index].set_items![0].text.includes('Unmodeled') ? ['Unmodeled drawback'] : [],
      })) };
    },
  });
  expect(result.weighted.map(stat => stat.id)).toEqual(['explicit.safe']);
  expect(result.scoreWarnings).toContain('unmodeled-candidates');
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

test('an indented PoB item with counted augments sets its actual current Sum without changing other gear', async () => {
  const current = `\n\t\tRarity: RARE\nEquipped\n${base.name}\nImplicits: 2\n{enchant}{rune}20 effect\n10 implicit\n25 flat\n\t\t`;
  const requests: EvaluateOptions[] = [];
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon2', text: current }, { slot: 'ring1', text: 'keep' }] },
    slot: 'weapon2', base, pool: [affix('flat')], itemLevel: 82, objective, combinations: false,
    evaluate: async options => {
      requests.push(options);
      return evaluator(text => 100 + Number(text.match(/(\d+) flat/)?.[1] ?? 0))(options);
    },
  });
  expect(requests.every(options => options.request.items?.[0].text === current)).toBe(true);
  expect(requests.flatMap(options => options.variants).every(variant => variant.set_items?.length === 1 && variant.set_items[0].slot === 'weapon2')).toBe(true);
  expect(result.currentItemScore).toMatchObject({ complete: true, score: 250 });
  expect(result.minimumWeight).toBe(250);
  expect(buildTradeQuery(result.weighted, { category: base.category, minimumWeight: result.minimumWeight }).query.stats[0]).toMatchObject({ value: { min: 250 } });
});

test('unknown current affixes disable a misleading minimum and constraints remain visibly outside Sum', async () => {
  const current = `Rarity: RARE\nEquipped\n${base.name}\n10 flat\nGrants a special buff`;
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon2', text: current }] },
    slot: 'weapon2', base, pool: [affix('flat')], itemLevel: 82,
    objective: { ...objective, constraints: [{ stat: 'TotalEHP', min: 100 }] }, combinations: false,
    evaluate: evaluator(text => 100 + (text.includes('flat') ? 10 : 0)),
  });
  expect(result.currentItemScore?.complete).toBe(false);
  expect(result.minimumWeight).toBe(0);
  expect(result.scoreWarnings).toContain('partial-current-score');
  expect(result.scoreWarnings).toContain('constraints-not-in-query');
});

/** Old published algorithm, retained only as a comparison oracle for these experiments. */
function oldWeights(blank: string, current: string, pool: TradeAffix[], score: (text: string) => number): WeightedStat[] {
  return pool.flatMap(mod => mod.stats).map(stat => ({ ...stat,
    weight: ((score(`${blank}\n${stat.line}`) - score(blank)) + (score(`${current}\n${stat.line}`) - score(current))) / 2 / stat.value,
  }));
}

test('benchmark: removal probes avoid the old capped-stat downgrade with the same six evaluations', async () => {
  const pool = [affix('accuracy'), affix('flat', 'suffix')];
  pool[0].lines = ['50 accuracy']; pool[0].stats[0] = { id: 'explicit.accuracy', line: '50 accuracy', value: 50 };
  const item = (accuracy: number, flat: number) => `Rarity: RARE\nEquipped\n${base.name}\n${accuracy} accuracy\n${flat} flat`;
  const value = (text: string, name: string) => [...text.matchAll(new RegExp(`(\\d+) ${name}`, 'g'))].reduce((sum, match) => sum + Number(match[1]), 0);
  const score = (text: string) => 100 + Math.min(value(text, 'accuracy'), 75) * 10 + value(text, 'flat');
  const current = item(70, 20);
  const candidates = [item(70, 50), item(0, 500)];
  const old = oldWeights(item(0, 0), current, pool, score);
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon2', text: current }] },
    slot: 'weapon2', base, pool, itemLevel: 82, objective, combinations: false,
    evaluate: async options => ({ ...(await evaluator(score)(options)), baseline: { TotalDPS: score(current), TotalEHP: 1000 } }),
  });
  const byWeights = (weights: WeightedStat[]) => [...candidates].sort((a, b) => scoreEquipment(b, weights).score - scoreEquipment(a, weights).score)[0];
  const oracle = Math.max(...candidates.map(score));
  expect(score(current)).toBe(820);
  expect(score(byWeights(old))).toBe(600);
  expect(score(byWeights(result.weighted))).toBe(850);
  expect(oracle - score(byWeights(old))).toBe(250);
  expect(oracle - score(byWeights(result.weighted))).toBe(0);
  expect(result.evaluated).toBe(6);
});

test('benchmark: local weapon interactions still require whole replacements; bounded combinations find the exhaustive best', async () => {
  const weapon: TradeBase = { ...base, name: 'Test Bow', category: 'weapon.bow', affix_limit: 1 };
  const pool = [affix('physical'), affix('speed', 'suffix')];
  const item = (physical: number, speed: number) => `Rarity: RARE\nEquipped\n${weapon.name}\n${physical} physical\n${speed} speed`;
  const value = (text: string, name: string) => [...text.matchAll(new RegExp(`(\\d+) ${name}`, 'g'))].reduce((sum, match) => sum + Number(match[1]), 0);
  const score = (text: string) => (10 + value(text, 'physical')) * (1 + value(text, 'speed') / 10);
  const current = item(10, 10);
  // The item with one huge roll beats two moderate rolls in either linear model,
  // while the complete local weapon calculation correctly prefers their product.
  const marketCandidates = [item(0, 25), item(10, 10)];
  const result = await optimizeTradeAffixes({ request: { items: [{ slot: 'weapon1', text: current }] },
    slot: 'weapon1', base: weapon, pool, itemLevel: 82, objective,
    evaluate: async options => ({ ...(await evaluator(score)(options)), baseline: { TotalDPS: score(current), TotalEHP: 1000 } }),
  });
  const old = oldWeights(item(0, 0), current, pool, score);
  const linearChoice = (weights: WeightedStat[]) => [...marketCandidates].sort((a, b) => scoreEquipment(b, weights).score - scoreEquipment(a, weights).score)[0];
  expect(score(linearChoice(old))).toBe(35);
  expect(score(linearChoice(result.weighted))).toBe(35);
  expect(Math.max(...marketCandidates.map(score))).toBe(40);
  expect(result.scoreWarnings).toEqual(expect.arrayContaining(['nonlinear', 'base-dependent']));
  const legalItems = [item(0, 0), item(10, 0), item(0, 10), item(10, 10)];
  expect(result.combinations[0].score).toBe(Math.max(...legalItems.map(score)));
  expect(result.combinations[0].score).toBe(40);
  expect(result.evaluated).toBe(9);
});

test('reference combinations exclude newly unmodeled drawbacks but retain baseline diagnostics', async () => {
  const risky = { ...affix('boost'), lines: ['10 boost', 'Unmodeled drawback'] };
  const result = await optimizeTradeAffixes({ request: {}, slot: 'weapon2', base, pool: [risky, affix('safe')],
    itemLevel: 82, objective, evaluate: async ({ variants }) => ({ baseline: { TotalDPS: 100 }, aborted: false,
      results: variants.map((variant, index) => {
        const text = variant.set_items![0].text;
        return { index, label: null, error: null, stats: { TotalDPS: 100 + (text.includes('boost') ? 1000 : 0) + (text.includes('safe') ? 20 : 0) },
          unsupported: ['Existing Guard', ...(text.includes('Unmodeled drawback') ? ['Unmodeled drawback'] : [])] };
      }) }),
  });
  expect(result.combinations.length).toBeGreaterThan(0);
  expect(result.combinations.every(entry => !entry.text.includes('Unmodeled drawback'))).toBe(true);
  expect(result.combinations[0].score).toBe(120);
  expect(result.unsupported).toEqual(['Existing Guard']);
  expect(result.scoreWarnings).toContain('unmodeled-candidates');
});
