import { describe, expect, test } from 'vitest';
import { explicitItemLines, itemForAffixProbes, scoreEquipment, withoutTradeStat } from './equipmentScore';
import { buildTradeQuery, tradeQueryWeights, type WeightedStat } from './trade';

const stats: WeightedStat[] = [
  { id: 'explicit.life', line: '+100 to maximum Life', value: 100, weight: 1.23456 },
  { id: 'explicit.fire', line: '+50% to Fire Resistance', value: 50, weight: 2.34567 },
  { id: 'explicit.flat', line: 'Adds 10 to 30 Physical Damage', value: 20, weight: 0.50123 },
];

describe('current equipment market Sum', () => {
  test('uses the exact rounded query weights and excludes implicit and augment sources', () => {
    const text = 'Rarity: RARE\nReference\nIron Ring\nItem Level: 80\nImplicits: 1\n+30% to Fire Resistance\n{enchant}{rune}+20 to maximum Life\n+75 to maximum Life\n+40% to Fire Resistance\nAdds 4 to 16 Physical Damage';
    const result = scoreEquipment(text, stats);
    const query = buildTradeQuery(stats, { category: 'accessory.ring' });
    const filters = query.query.stats[0].filters as { id: string; value: { weight: number } }[];
    const actualValues: Record<string, number> = { 'explicit.life': 75, 'explicit.fire': 40, 'explicit.flat': 10 };
    const officialSum = filters.reduce((sum, filter) => sum + actualValues[filter.id] * filter.value.weight, 0);
    expect(result.score).toBe(officialSum);
    expect(result.complete).toBe(true);
    expect(result.totalLines).toBe(3);
    expect(result.contributions.map(row => row.value)).toEqual([75, 40, 10]);
  });

  test('clipboard sections and fractured explicit lines retain their proper namespace', () => {
    const text = 'Item Class: Rings\nRarity: RARE\nReference\nIron Ring\n--------\nRequirements:\nLevel: 60\n--------\nItem Level: 80\n--------\n+20% to Fire Resistance (implicit)\n--------\n+70 to maximum Life (fractured)\n+35% to Fire Resistance\n--------\nCorrupted';
    expect(scoreEquipment(text, stats)).toMatchObject({ complete: true, matchedLines: 2, totalLines: 2,
      score: 70 * 1.235 + 35 * 2.346 });
  });

  test('game and catalog capitalization differences preserve the same range value and removal probe', () => {
    const attack = { id: 'explicit.attack', line: 'Adds 29 to 45 Fire damage to Attacks', value: 37, weight: 2 };
    const text = 'Rarity: RARE\nReference\nIron Ring\nAdds 10 TO 20 Fire Damage to Attacks';
    expect(scoreEquipment(text, [attack])).toMatchObject({ complete: true, score: 30 });
    expect(withoutTradeStat(text, attack, [attack])).toEqual({ text: 'Rarity: RARE\nReference\nIron Ring', value: 15 });
  });

  test('does not misrepresent missing or ambiguous mappings as complete scores', () => {
    const text = 'Rarity: RARE\nReference\nIron Ring\n+70 to maximum Life\nGrants a special buff\nAdds (2-5) to (7-12) Physical Damage';
    const result = scoreEquipment(text, stats);
    expect(result.complete).toBe(false);
    expect(result.matchedLines).toBe(1);
    expect(result.unscoredLines).toHaveLength(2);
    const ambiguous = [...stats, { ...stats[0], id: 'explicit.other-life' }];
    expect(scoreEquipment('Rarity: RARE\nReference\nIron Ring\n+70 to maximum Life', stats, ambiguous).complete).toBe(false);
  });

  test('excluded query weights and signed inverse stats use the same final query units', () => {
    const inverse = { id: 'explicit.cost', line: '10% reduced Mana Cost', value: -10, weight: -2.12345 };
    const weights = [{ ...stats[0], weight: 0.0001 }, inverse, { ...inverse, weight: -90 }];
    const result = scoreEquipment('Rarity: RARE\nReference\nIron Ring\n+70 to maximum Life\n5% reduced Mana Cost', weights, [...stats, inverse]);
    expect(tradeQueryWeights(weights)).toEqual([{ ...inverse, weight: -2.123 }]);
    expect(result.complete).toBe(true);
    expect(result.score).toBeCloseTo(10.615, 9);
  });

  test('reference score applies the same final 32-filter cap as the search query', () => {
    const weights = Array.from({ length: 35 }, (_, i) => ({ id: `explicit.stat_${i}`, line: `${i + 1} effect ${String.fromCharCode(65 + Math.floor(i / 26), 65 + i % 26)}`, value: i + 1, weight: 1 }));
    const text = ['Rarity: RARE', 'Reference', 'Iron Ring', ...weights.map(stat => stat.line)].join('\n');
    expect(scoreEquipment(text, weights).score).toBe(32 * 33 / 2);
    expect(buildTradeQuery(weights, { category: 'accessory.ring' }).query.stats[0].filters).toHaveLength(32);
  });
});

test('removes only the probed explicit effect; rolled defence is not reused after local affix changes', () => {
  const text = 'Rarity: RARE\nReference\nIron Ring\nQuality: 20\nArmour: 600\nImplicits: 1\n+20% to Fire Resistance\n+30% to Fire Resistance\n+70 to maximum Life';
  const result = withoutTradeStat(itemForAffixProbes(text), stats[1], stats)!;
  expect(result.value).toBe(30);
  expect(result.text).toContain('+20% to Fire Resistance');
  expect(result.text).toContain('+70 to maximum Life');
  expect(result.text).toContain('Quality: 20');
  expect(result.text).not.toContain('+30% to Fire Resistance');
  expect(result.text).not.toContain('Armour: 600');
  expect(explicitItemLines(result.text).map(row => row.line)).toEqual(['+70 to maximum Life']);
});
