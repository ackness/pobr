import { describe, expect, test } from 'vitest';
import { explicitItemLines, itemForAffixProbes, scoreEquipment, withoutTradeStat } from './equipmentScore';
import { buildTradeQuery, tradeQueryWeights, type WeightedStat } from './trade';

const stats: WeightedStat[] = [
  { id: 'explicit.life', line: '+100 to maximum Life', value: 100, weight: 1.23456 },
  { id: 'explicit.fire', line: '+50% to Fire Resistance', value: 50, weight: 2.34567 },
  { id: 'explicit.flat', line: 'Adds 10 to 30 Physical Damage', value: 20, weight: 0.50123 },
];

describe('current equipment market Sum', () => {
  test('PoB XML indentation does not make a fully mapped item score incomplete', () => {
    const text = '\n\t\tRarity: RARE\nReference\nIron Ring\n+70 to maximum Life\n\t\t';
    expect(scoreEquipment(text, stats)).toMatchObject({ complete: true, matchedLines: 1, totalLines: 1, score: 70 * 1.235 });
    expect(scoreEquipment('+70 to maximum Life', stats).complete).toBe(false);
  });

  test('counted rune and enchant lines leave every following explicit affix available for scoring and removal', () => {
    const text = 'Rarity: RARE\nReference\nVile Robe\nImplicits: 3\n{enchant}{rune}+50 to maximum Life\n--------\nNote: retained metadata\n{enchant}+60 to maximum Life\n+30% to Fire Resistance (implicit)\n+75 to maximum Life\n+40% to Fire Resistance';
    expect(explicitItemLines(text).map(row => row.line)).toEqual(['+75 to maximum Life', '+40% to Fire Resistance']);
    expect(scoreEquipment(text, stats)).toMatchObject({ complete: true, matchedLines: 2, totalLines: 2,
      score: 75 * 1.235 + 40 * 2.346 });
    const removed = withoutTradeStat(text, stats[0], stats)!;
    expect(removed.value).toBe(75);
    expect(removed.text).toContain('{enchant}{rune}+50 to maximum Life');
    expect(removed.text).toContain('{enchant}+60 to maximum Life');
    expect(removed.text).not.toContain('+75 to maximum Life');
  });

  test('belt charm-slot metadata does not disable the current-item minimum', () => {
    const text = 'Rarity: RARE\nReference\nDouble Belt\nCharm Slots: 3\nImplicits: 1\nHas 3 Charm Slots\n+70 to maximum Life';
    expect(scoreEquipment(text, stats)).toMatchObject({ complete: true, matchedLines: 1, totalLines: 1, score: 70 * 1.235 });
  });

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

test('a wrapped official stat is counted and removed once, including crafted exports and clipboard lines', () => {
  const compound = { id: 'explicit.compound', line: 'Inflict Anaemia on Hit Anaemia allows +3 Corrupted Blood debuffs to be inflicted on enemies',
    source_lines: ['Inflict Anaemia on Hit', 'Anaemia allows +3 Corrupted Blood debuffs to be inflicted on enemies'], value: 3, weight: 2 };
  const header = 'Rarity: RARE\nReference\nStellar Amulet';
  const wrapped = `${header}\n{crafted}Inflict Anaemia on Hit\n{crafted}Anaemia allows +2 Corrupted Blood debuffs to be inflicted on enemies\n+70 to maximum Life`;
  const result = scoreEquipment(wrapped, [...stats, compound]);
  expect(result).toMatchObject({ complete: true, totalLines: 2, matchedLines: 2 });
  expect(result.contributions.filter(row => row.id === compound.id)).toEqual([
    expect.objectContaining({ value: 2, score: 4 }),
  ]);
  expect(withoutTradeStat(wrapped, compound, [...stats, compound])).toEqual({ text: `${header}\n+70 to maximum Life`, value: 2 });
  expect(scoreEquipment(`${header}\n${compound.line} (crafted)`, [compound])).toMatchObject({ complete: true, score: 6 });
  expect(scoreEquipment(`${header}\nInflict Anaemia on Hit`, [compound]).complete).toBe(false);
});

test('conditional constants, flags and official wording keep the correct trade units', () => {
  const condition = { id: 'explicit.conditional', line: 'Every 4 seconds, gain 20% increased Damage', value: 20, value_indices: [1], weight: 3 };
  const flag = { id: 'explicit.flag', line: "Your Hits can't be Evaded", value: 1, value_indices: [], weight: 5 };
  const alias = { id: 'explicit.reservation', line: '12% increased Spirit Reservation Efficiency',
    trade_line: '12% increased Spirit Reservation Efficiency of Skills', value: 12, weight: 4 };
  const header = 'Rarity: RARE\nReference\nVile Robe';
  const text = `${header}\nEvery 4 seconds, gain 10% increased Damage\nYour Hits can't be Evaded\n{desecrated}8% increased Spirit Reservation Efficiency of Skills`;
  expect(scoreEquipment(text, [condition, flag, alias])).toMatchObject({ complete: true, score: 67 });
  expect(scoreEquipment(text.replace('Every 4', 'Every 8'), [condition, flag, alias]).complete).toBe(false);
  expect(withoutTradeStat(text, flag, [condition, flag, alias])?.value).toBe(1);
  const rolledFlag = { ...flag, line: '10% chance to Daze on Hit', trade_line: 'Dazes on Hit' };
  expect(scoreEquipment(`${header}\n7% chance to Daze on Hit`, [rolledFlag])).toMatchObject({ complete: true, score: 5 });
});

test('corruption enchant weights exclude socket runes and removal updates the implicit count', () => {
  const enchant = { ...stats[0], id: 'enchant.life', weight: 2 };
  const text = 'Rarity: RARE\nReference\nIron Ring\nRunic Ward: 50\nImplicits: 4\n{enchant}Allocates Ancestral Reach\n{enchant}{rune}+20 to maximum Life\n{enchant}+30 to maximum Life\n+10% to Fire Resistance\n{crafted}+70 to maximum Life';
  const weighted = [...stats, enchant];
  expect(scoreEquipment(text, weighted)).toMatchObject({ complete: true, score: 60 + 70 * 1.235 });
  const removed = withoutTradeStat(text, enchant, weighted)!;
  expect(removed.value).toBe(30);
  expect(removed.text).toContain('Implicits: 3');
  expect(removed.text).toContain('{enchant}{rune}+20 to maximum Life');
  expect(scoreEquipment(removed.text, weighted)).toMatchObject({ complete: true, score: 70 * 1.235 });
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
