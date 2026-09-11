import { expect, test, vi } from 'vitest';
import { compareReplacement, normalizeCopiedItem, validateReplacement } from './itemReplacement';
import type { TradeCatalog } from './tradeOptimizer';
import type { CalculateBuildRequest } from '../api/types';

const catalog: TradeCatalog = { mods: [], bases: [
  { name: 'Sapphire Ring', category: 'accessory.ring', tags: [], level: 12, implicits: [] },
  { name: 'Twin Bow', category: 'weapon.bow', tags: [], level: 55, implicits: [] },
  { name: 'Primed Quiver', category: 'armour.quiver', tags: [], level: 30, implicits: [] },
  { name: 'Ruby', category: 'jewel', tags: [], level: 1, implicits: [] },
] };
const copied = 'Rarity: RARE\nUpgrade\nSapphire Ring\n--------\nRequirements:\nLevel: 60\nDex: 90\n--------\nItem Level: 100\n--------\n+40 to maximum Life';
const request: CalculateBuildRequest = { character: { level: 72, class_name: 'Ranger' },
  items: [{ slot: 'ring1', text: 'Rarity: NORMAL\nSapphire Ring' }, { slot: 'ring2', text: 'Keep this ring' }],
  allocated_nodes: [12], jewels: [{ socket_node: 12, text: 'Keep this jewel' }],
};
const translate = async (lines: string[]) => lines.map(line => line === 'Sapphire Ring' ? '蓝玉戒指' : line);

test('clipboard CN headers retain effects and map short attribute requirements', () => {
  const normalized = normalizeCopiedItem('物品类别: 戒指\n稀有度：稀有\n升级\n蓝玉戒指\n--------\n需求等级: 60\n力量: 10\n敏捷: 20\n智慧: 30\n--------\n+40 最大生命');
  expect(normalized).toContain('Rarity: RARE\n升级\n蓝玉戒指');
  expect(normalized).toContain('Level: 60\nStr: 10\nDex: 20\nInt: 30');
  expect(normalized).toContain('+40 最大生命');
});

test('required level uses declared/base requirements, not item level', async () => {
  await expect(validateReplacement(request, 'ring1', copied, catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', copied.replace('Level: 60', 'Level: 80'), catalog, translate)).rejects.toThrow('item-level');
  await expect(validateReplacement({ ...request, character: { level: 1 } }, 'ring1', 'Rarity: NORMAL\nSapphire Ring', catalog, translate)).rejects.toThrow('item-level');
});

test('unknown bases, wrong slots, inactive jewel sockets and unidentified items never reach calc', async () => {
  for (const [slot, text, error] of [
    ['helmet', copied, 'wrong-slot'], ['ring1', 'Rarity: RARE\nSapphire Ring\nTwin Bow', 'wrong-slot'], ['ring1', copied.replace('Sapphire Ring', 'Unknown'), 'unknown-base'],
    ['Jewel@13', 'Rarity: MAGIC\nRuby', 'wrong-slot'], ['ring1', `${copied}\nUnidentified`, 'unidentified-item'],
  ]) {
    const evaluate = vi.fn();
    await expect(compareReplacement(request, slot, text, undefined, { catalog, translate, evaluate })).rejects.toThrow(error);
    expect(evaluate).not.toHaveBeenCalled();
  }
});

test('magic and localized base names are verified without interpreting modifier lines as bases', async () => {
  await expect(validateReplacement(request, 'ring1', 'Rarity: MAGIC\nHealthy Sapphire Ring of the Beast', catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', copied.replace('Sapphire Ring', '蓝玉戒指'), catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', 'Rarity: RARE\nFake\nUnknown\n--------\nSapphire Ring', catalog, translate)).rejects.toThrow('unknown-base');
});

test('quiver needs a bow and coordinated weapon changes are rejected', async () => {
  await expect(validateReplacement(request, 'weapon2', 'Rarity: NORMAL\nPrimed Quiver', catalog, translate)).rejects.toThrow('weapon-type');
  await expect(validateReplacement({ ...request, items: [{ slot: 'weapon1', text: 'Rarity: NORMAL\nTwin Bow' }] }, 'weapon2', 'Rarity: NORMAL\nPrimed Quiver', catalog, translate)).resolves.toEqual(catalog.bases[2]);
});

test('preview substitutes only the requested position and preserves the source build', async () => {
  const original = structuredClone(request);
  const evaluate = vi.fn(async () => ({ baseline: { TotalDPS: 100, TotalEHP: 1000 },
    results: [{ index: 0, label: null, stats: { TotalDPS: 90, TotalEHP: 1200 }, unsupported: ['Guard'], error: null }], aborted: false }));
  const result = await compareReplacement(request, 'ring1', copied, undefined, { catalog, translate, evaluate });
  expect(evaluate).toHaveBeenCalledWith({ request, signal: undefined, variants: [{ set_items: [{ slot: 'ring1', text: copied }] }] });
  expect(result.stats.TotalDPS).toBe(90);
  expect(result.unsupported).toEqual(['Guard']);
  expect(request).toEqual(original);
});

test('cancelled comparisons do not evaluate or return stale results', async () => {
  const controller = new AbortController(); controller.abort();
  const evaluate = vi.fn();
  await expect(compareReplacement(request, 'ring1', copied, controller.signal, { catalog, translate, evaluate })).rejects.toThrow();
  expect(evaluate).not.toHaveBeenCalled();
});
