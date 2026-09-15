import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { categoryAffixPool, categorySearchMods, type TradeCatalog } from './tradeOptimizer';
import { scoreEquipment } from './equipmentScore';
import { editableAffixPool } from './marketAffixEditor';

const catalog = JSON.parse(readFileSync(new URL('../../../data/4.5.5.2/overlay/trade_catalog.json', import.meta.url), 'utf8')) as TradeCatalog;
const special = (id: string) => catalog.search_mods!.find(mod => mod.id === id)!;

test('GGG alloy recipes supply both hybrid components on the correct caster categories', () => {
  const alloy = special('alloy:AlloyCastSpeedDamageAsExtraColdHybridOneHand1');
  expect(alloy.categories).toEqual(['armour.focus', 'weapon.wand']);
  expect(alloy.stats.map(stat => stat.id).sort()).toEqual(['explicit.stat_1158842087', 'explicit.stat_2891184298']);
  expect(special('alloy:AlloyCastSpeedDamageAsExtraColdHybrid1').categories).toEqual(['weapon.staff']);
  const wand = catalog.bases.find(base => base.category === 'weapon.wand')!;
  expect(editableAffixPool(catalog, wand).some(mod => mod.id.endsWith('AlloyCastSpeedDamageAsExtraColdHybridOneHand1'))).toBe(false);
  expect(categorySearchMods(catalog, 'weapon.wand', alloy.level - 1).some(mod => mod.id === alloy.id)).toBe(false);
});

test('special sources retain explicit hashes, corruption enchant hashes and the full wrapped hash', () => {
  expect(new Set(catalog.search_mods!.map(mod => mod.source))).toEqual(new Set(['alloy', 'essence', 'desecrated', 'breach', 'influence', 'corrupted']));
  for (const mod of catalog.search_mods!) {
    expect(mod.stats.every(stat => stat.id.startsWith(mod.source === 'corrupted' ? 'enchant.stat_' : 'explicit.stat_'))).toBe(true);
  }
  expect(special('essence:EssencePercentStrength1').categories).toEqual(['accessory.amulet']);
  const compound = special('breach:GenesisTreeAmuletAnaemiaOnHitCrafted');
  expect(compound.stats).toHaveLength(1);
  expect(compound.stats[0]).toMatchObject({ id: 'explicit.stat_971590056', value: 3, source_lines: compound.lines });
  const score = scoreEquipment(`Rarity: RARE\nReference\nStellar Amulet\n${compound.lines.join('\n')}`, compound.stats.map(stat => ({ ...stat, weight: 2 })));
  expect(score).toMatchObject({ complete: true, score: 6, totalLines: 1 });
});

test.each([
  ['weapon.wand', 'Attuned Wand', ['80% increased Elemental Damage', '{crafted}Gain 13% of Elemental Damage as Extra Cold Damage']],
  ['armour.chest', 'Vile Robe', ['{desecrated}12% increased Spirit Reservation Efficiency of Skills', '+95 to maximum Energy Shield']],
  ['accessory.amulet', 'Stellar Amulet', ['{desecrated}58% increased Energy Shield from Equipped Body Armour', '10% increased Strength']],
  ['jewel', 'Sapphire', ['8% increased Cold Damage', 'Damage Penetrates 6% Cold Resistance']],
] as const)('special current rolls are fully mapped in %s', (category, base, lines) => {
  const templates = [...categoryAffixPool(catalog, category), ...categorySearchMods(catalog, category)].flatMap(mod => mod.stats);
  const result = scoreEquipment(`Rarity: RARE\nReference\n${base}\n${lines.join('\n')}`, templates.map(stat => ({ ...stat, weight: 1 })), templates);
  expect(result.unscoredLines).toEqual([]);
  expect(result.complete).toBe(true);
  expect(result.matchedLines).toBe(2);
});
