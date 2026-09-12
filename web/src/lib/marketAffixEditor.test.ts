import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import type { ItemLineJson } from '../api/types';
import { affixRollLines, affixTier, availableAffixes, buildAffixItem, createAffixDraft, editableAffixPool, validateAffixDraft } from './marketAffixEditor';
import type { TradeAffix, TradeBase, TradeCatalog } from './tradeOptimizer';

const base: TradeBase = { name: 'Sapphire Ring', category: 'accessory.ring', tags: ['ring', 'default'], level: 10, implicits: [], domain: 'equipment', affix_limit: 3 };
const mod = (id: string, lines: string[], extra: Partial<TradeAffix> = {}): TradeAffix => ({ id, group: id, kind: 'prefix', level: 1,
  lines: lines.map(line => line.replace(/\((\d+)-(\d+)\)/g, '$2')), roll_lines: lines, weights: [['ring', 1]], stats: [], domain: 'equipment', ...extra });
const life = mod('life-low', ['+(70-84) to maximum Life'], { level: 38, group: 'life' });
const lifeHigh = mod('life-high', ['+(100-119) to maximum Life'], { level: 54, group: 'life' });
const fire = mod('fire', ['+(31-35)% to Fire Resistance'], { kind: 'suffix', level: 50 });
const damage = mod('damage', ['Adds (2-6) to (20-30) Fire Damage to Attacks']);
const hybrid = mod('hybrid', ['(10-20)% increased Armour', '+(10-15) to maximum Life']);
const pool = [life, lifeHigh, fire, damage, hybrid];
const text = (lines: string[], rarity = 'RARE', properties: string[] = []) => [
  `Rarity: ${rarity}`, ...(rarity === 'RARE' ? ['Listed Ring'] : []), base.name,
  'Item Level: 80', ...properties, 'Implicits: 1', '+20% to Cold Resistance', ...lines,
].join('\n');
const classify = (input: string): ItemLineJson[] => {
  const raw = input.split('\n');
  const start = raw.findIndex(line => line.startsWith('Implicits:')) + 1;
  return raw.slice(1).map((line, i) => ({ text: line.replace(/\{[^}]*\}/g, ''),
    kind: i + 1 === start ? 'implicit' : i + 1 > start ? line.includes('{rune}') ? 'rune' : 'explicit' : 'struct' }));
};
const draft = (lines: string[], rarity?: string, props?: string[]) => {
  const input = text(lines, rarity, props);
  return createAffixDraft(input, base, classify(input), pool);
};

test('tier ranges retain independent damage endpoints and exact fractional midpoints', () => {
  expect(affixRollLines(lifeHigh)).toEqual(['+109.5 to maximum Life']);
  expect(affixRollLines(damage)).toEqual(['Adds 4 to 25 Fire Damage to Attacks']);
  expect(affixRollLines(mod('negative', ['(-9--3)% reduced Enemy Resistance']))).toEqual(['-6% reduced Enemy Resistance']);
  expect(() => affixRollLines(life, NaN)).toThrow('invalid-roll');
  expect(() => affixRollLines({ ...life, roll_lines: undefined })).toThrow('missing-ranges');
});

test('first matching spawn weight, base domain and all base-compatible tiers are respected', () => {
  const otherBase = mod('blocked', ['+10 to maximum Mana'], { weights: [['ring', 0], ['default', 100]] });
  const jewel = mod('jewel', ['+10 to maximum Mana'], { domain: 'jewel' });
  const eligible = editableAffixPool({ bases: [base], mods: [...pool, otherBase, jewel] }, base);
  expect(eligible.map(row => row.id)).toEqual(pool.map(row => row.id));
  expect(affixTier(lifeHigh, eligible)).toBe(1);
  expect(affixTier(life, eligible)).toBe(2);
});

test('existing rolls resolve by numeric tier range, while hybrids consume a single affix slot', () => {
  const value = draft(['15% increased Armour', '+12 to maximum Life', '+33% to Fire Resistance']);
  expect(value.rows.map(row => row.modId)).toEqual(['hybrid', 'fire']);
  expect(value.rows[0].original).toEqual(['15% increased Armour', '+12 to maximum Life']);
  expect(value.rows[0].indices).toHaveLength(2);
  expect(validateAffixDraft(value, pool, 80)).toBeUndefined();
  expect(availableAffixes(value, pool, 80).map(row => row.id)).toEqual(['life-low', 'life-high', 'damage']);
});

test('translated templates match copied rolls without changing unknown or augment text', () => {
  const raw = text(['+80 最大生命', '{rune}+12 to maximum Life', 'Unmodeled market effect: opaque']);
  const value = createAffixDraft(raw, base, classify(raw), pool, new Map([[life.lines[0], '+84 最大生命']]));
  expect(value.rows.map(row => row.modId)).toEqual(['life-low', undefined]);
  expect(value.rows[1].original).toEqual(['Unmodeled market effect: opaque']);
  expect(value.text).toBe(raw);
  expect(availableAffixes(value, pool, 80)).toHaveLength(0);
  expect(() => buildAffixItem(value, pool, 80)).toThrow('unknown');
});

test('ambiguous overlapping flat/hybrid effects never invent free prefix or suffix slots', () => {
  const armour = mod('armour', ['(10-20)% increased Armour']);
  const smallLife = mod('small-life', ['+(10-15) to maximum Life']);
  const raw = text(['15% increased Armour', '+12 to maximum Life']);
  const value = createAffixDraft(raw, base, classify(raw), [hybrid, armour, smallLife]);
  expect(value.rows.every(row => !row.modId)).toBe(true);
  expect(availableAffixes(value, [hybrid, armour, smallLife], 80)).toEqual([]);
});

test('three merged stat sources are unresolved even when no two-source interval reaches the copied total', () => {
  const sources = [mod('high', ['+60 to maximum Life']), ...['a', 'b', 'c'].map(id => mod(id, ['+20 to maximum Life']))];
  const raw = text(['+60 to maximum Life']);
  expect(createAffixDraft(raw, base, classify(raw), sources).rows[0].modId).toBeUndefined();
  const magic = text(['+60 to maximum Life'], 'MAGIC');
  expect(createAffixDraft(magic, base, classify(magic), sources).rows[0].modId).toBe('high');
});

test('magic capacity, mod-group exclusivity, item level and character requirements all constrain choices', () => {
  const value = draft(['+80 to maximum Life'], 'MAGIC');
  value.itemLevel = 53;
  expect(availableAffixes(value, pool, 80).map(row => row.id)).toEqual(['fire']);
  expect(availableAffixes(value, pool, 39)).toEqual([]);
  const replacing = availableAffixes(value, pool, 80, value.rows[0].key);
  expect(replacing.map(row => row.id)).not.toContain('life-high');
  value.itemLevel = 54;
  expect(availableAffixes(value, pool, 43, value.rows[0].key).map(row => row.id)).toContain('life-high');
  expect(availableAffixes(value, pool, 42, value.rows[0].key).map(row => row.id)).not.toContain('life-high');
  value.rows.push({ key: 'invalid', modId: 'damage', indices: [], original: [], roll: 0.5, edited: true });
  expect(validateAffixDraft(value, pool, 80)).toBe('affix-limit');
});

test('a missing item level must be supplied, and corrupted/unique items are not ordinary crafting targets', () => {
  const raw = text(['+80 to maximum Life']).replace('Item Level: 80\n', '');
  const value = createAffixDraft(raw, base, classify(raw), pool);
  expect(value.itemLevel).toBeNull();
  expect(availableAffixes(value, pool, 80)).toEqual([]);
  expect(() => buildAffixItem(value, pool, 80)).toThrow('item-level');
  const corrupted = `${raw}\nCorrupted`;
  expect(createAffixDraft(corrupted, base, classify(corrupted), pool).locked).toBe('corrupted');
  const unique = raw.replace('RARE', 'UNIQUE');
  expect(createAffixDraft(unique, base, classify(unique), pool).locked).toBe('rarity');
});

test.each(['Twice Corrupted', 'Sanctified', '已腐化', '已镜像', '已鏡像', '未鉴定', '未鑑定'])('the %s item state blocks tier changes and ordinary crafting', state => {
  const raw = `${text(['+80 to maximum Life'])}\n${state}`;
  const value = createAffixDraft(raw, base, classify(raw), pool);
  expect(value.locked).toBe('corrupted');
  expect(availableAffixes(value, pool, 80)).toEqual([]);
  value.rows = [];
  expect(() => buildAffixItem(value, pool, 80)).toThrow('corrupted');
  expect(value.text).toBe(raw);
});

test('complete item rebuild preserves untouched affixes and augments, removes deleted rows and stale local property totals', () => {
  const value = draft(['+80 to maximum Life', '+33% to Fire Resistance', '{rune}10% increased Armour'], 'RARE', ['Armour: 456', 'Quality: +20%', 'Sockets: S', 'Rune: Iron Rune', 'LevelReq: 40']);
  value.rows = value.rows.filter(row => row.modId !== 'fire');
  value.rows[0] = { ...value.rows[0], modId: lifeHigh.id, edited: true, roll: 0.5 };
  const rebuilt = buildAffixItem(value, pool, 80);
  expect(rebuilt).toContain('+109.5 to maximum Life');
  expect(rebuilt).not.toContain('Fire Resistance');
  expect(rebuilt).not.toContain('Armour: 456');
  expect(rebuilt).toContain('Implicits: 1\n+20% to Cold Resistance');
  expect(rebuilt).toContain('Quality: +20%\nSockets: S\nRune: Iron Rune');
  expect(rebuilt).toContain('{rune}10% increased Armour');
  expect(rebuilt).toContain('LevelReq: 43');
  expect(value.text).toContain('+80 to maximum Life');
  expect(value.text).toContain('+33% to Fire Resistance');
});

test('unknown source rows are removed only after the player explicitly removes or replaces them', () => {
  const value = draft(['Opaque modifier']);
  value.rows = [{ ...value.rows[0], modId: life.id, roll: 0.5, edited: true }];
  expect(buildAffixItem(value, pool, 80)).toContain('+77 to maximum Life');
  expect(buildAffixItem(value, pool, 80)).not.toContain('Opaque modifier');
});

test('fractured modifiers remain fixed while still consuming their real affix slot', () => {
  const value = draft(['{fractured}+80 to maximum Life']);
  expect(value.rows[0].fractured).toBe(true);
  expect(availableAffixes(value, pool, 80, value.rows[0].key)).toEqual([]);
  expect(availableAffixes(value, pool, 80).map(row => row.id)).not.toContain('life-high');
  expect(buildAffixItem(value, pool, 80)).toContain('{fractured}+80 to maximum Life');
  value.rows = [];
  expect(() => buildAffixItem(value, pool, 80)).toThrow('fractured');
});

test('the pinned PoB2 export has ranges for every tier and agrees with all existing maximum-roll lines', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../../data/4.5.5.2/overlay/trade_catalog.json', import.meta.url), 'utf8')) as TradeCatalog;
  expect(catalog.mods.length).toBeGreaterThan(3000);
  for (const entry of catalog.mods) expect(affixRollLines(entry, 1)).toEqual(entry.lines);
  const ring = catalog.bases.find(entry => entry.name === 'Sapphire Ring')!;
  const candidates = editableAffixPool(catalog, ring);
  const input = text(['+80 to maximum Life']);
  const actual = createAffixDraft(input, ring, classify(input), candidates);
  expect(actual.rows[0].modId).toBe('equipment:IncreasedLife6');
  expect(affixRollLines(candidates.find(entry => entry.id === 'equipment:IncreasedLife8')!)).toEqual(['+109.5 to maximum Life']);
});

test('merged physical and accuracy rolls from three real bow prefixes never become two confirmed prefixes', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../../data/4.5.5.2/overlay/trade_catalog.json', import.meta.url), 'utf8')) as TradeCatalog;
  const bow = catalog.bases.find(entry => entry.name === 'Twin Bow')!;
  const candidates = editableAffixPool(catalog, bow);
  const physical = candidates.find(entry => entry.id === 'equipment:LocalIncreasedPhysicalDamagePercent2')!;
  const hybrid = candidates.find(entry => entry.id === 'equipment:LocalIncreasedPhysicalDamagePercentAndAccuracyRating5')!;
  const accuracy = candidates.find(entry => entry.id === 'equipment:LocalIncreasedAccuracy1')!;
  expect(affixRollLines(physical, 0)).toEqual(['50% increased Physical Damage']);
  expect(affixRollLines(hybrid, 0)).toEqual(['45% increased Physical Damage', '+98 to Accuracy Rating']);
  expect(affixRollLines(accuracy, 0)).toEqual(['+11 to Accuracy Rating']);
  expect(affixRollLines(accuracy, 1)).toEqual(['+32 to Accuracy Rating']);
  // Legal source rolls: 50 physical + (50 physical / 100 accuracy) + 20 accuracy.
  // The market merges these three prefixes into two display lines.
  const raw = ['Rarity: RARE', 'Merged Prefix Bow', 'Twin Bow', 'Item Level: 80', 'Implicits: 0',
    '100% increased Physical Damage', '+120 to Accuracy Rating'].join('\n');
  const classified: ItemLineJson[] = raw.split('\n').slice(1).map((line, index) => ({ text: line, kind: index >= 4 ? 'explicit' : 'struct' }));
  const value = createAffixDraft(raw, bow, classified, candidates);
  expect(value.rows.map(row => row.modId)).toEqual([undefined, undefined]);
  expect(value.rows.flatMap(row => row.original)).toEqual(['100% increased Physical Damage', '+120 to Accuracy Rating']);
  expect(availableAffixes(value, candidates, 80)).toEqual([]);
  expect(() => buildAffixItem(value, candidates, 80)).toThrow('unknown');

  const aliases = new Map(candidates.flatMap(entry => entry.lines.map(line => [line, line
    .replace(/(\d+)% increased Physical Damage/, '物理伤害提高 $1%')
    .replace(/\+(\d+) to Accuracy Rating/, '+$1 命中值')] as const)));
  const localized = raw.replace('100% increased Physical Damage', '物理伤害提高 100%').replace('+120 to Accuracy Rating', '+120 命中值');
  const chinese = classified.map(line => ({ ...line, text: line.text === '100% increased Physical Damage' ? '物理伤害提高 100%' : line.text === '+120 to Accuracy Rating' ? '+120 命中值' : line.text }));
  expect(createAffixDraft(localized, bow, chinese, candidates, aliases).rows.every(row => !row.modId)).toBe(true);
});
