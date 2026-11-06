import { expect, it } from 'vitest';
import { linesToTranslate, localizeStructLine, localizedLines } from './useLocalizedLines';
import { itemDisplaySections } from '../lib/itemDisplay';

it.each([
  ['Armour', '护甲', '護甲'], ['Evasion', '闪避', '閃避'], ['Evasion Rating', '闪避值', '閃避值'],
  ['Energy Shield', '能量护盾', '能量護盾'], ['Ward', '结界', '結界'],
  ['Physical Damage', '物理伤害', '物理傷害'], ['Block Chance', '格挡几率', '格擋機率'],
])('localizes the %s item property only at the display boundary', (key, cn, tw) => {
  const original = `${key}: 431`;
  for (const [lang, label] of [['zh-CN', cn], ['zh-TW', tw], ['en-US', key]] as const) {
    const display = localizeStructLine(original, lang) ?? original;
    expect(display).toBe(`${label}: 431`);
    const lines = Object.freeze([Object.freeze({ kind: 'struct' as const, text: original })]);
    expect(itemDisplaySections(lines, [display]).properties).toEqual([{ label, value: '431', index: 0 }]);
    expect(itemDisplaySections(lines, [display]).metadata).toEqual([]);
    expect(lines[0].text).toBe(original);
  }
});

it('leaves modifier lines, unknown properties, numeric formatting and English source intact', () => {
  for (const lang of ['zh-CN', 'zh-TW', 'en-US'] as const) {
    expect(localizeStructLine('Future Defence: 431', lang)).toBeNull();
    expect(localizeStructLine('36% increased Evasion Rating', lang)).toBeNull();
  }
  expect(localizeStructLine('Evasion Rating: 1,234 (augmented)', 'zh-CN')).toBe('闪避值: 1,234 (augmented)');
  expect(localizeStructLine('Evasion Rating: 431', 'en-US')).toBeNull();
});

it('matches translations by position, so one line can be covered in one place and stand alone in another', () => {
  const lines = ['Item Level: 83', 'All Damage is taken from Mana before Life', '50% less Mana Recovery Rate', '', '50% less Mana Recovery Rate'];
  // Locally translated and blank lines are sent blank so they still separate their neighbours.
  expect(linesToTranslate(lines, 'zh-CN')).toEqual(['', lines[1], lines[2], '', lines[4]]);
  const out = [undefined, '受到的所有伤害在扣除生命前优先从魔力扣除', '', undefined, '魔力回复速度总降 50%'];
  expect(localizedLines(lines, 'zh-CN', out))
    .toEqual(['物品等级: 83', '受到的所有伤害在扣除生命前优先从魔力扣除', '', '', '魔力回复速度总降 50%']);
  expect(localizedLines(['Rune: Iron Rune'], 'zh-CN', [''])).toEqual(['符文: Iron Rune']);
});

it('returns the en-US lines themselves, so consumers memoizing on them do not recompute', () => {
  const lines = ['+5 to Strength', 'Item Level: 83'];
  expect(localizedLines(lines, 'en-US', [])).toBe(lines);
});

it('takes a Bonded line from the language pack, whole first, else around the pack\'s own prefix', () => {
  const lines = ['Bonded: +40 to maximum Life', 'Item Level: 83', 'Bonded: Grants Skill: Level 20 Foo', 'Bonded: Unknown words'];
  // Each Bonded line goes whole; its effect and the prefix probe follow for the fallback.
  const sent = linesToTranslate(lines, 'zh-CN');
  expect(sent).toEqual([lines[0], '', lines[2], lines[3],
    '', '+40 to maximum Life', '', 'Grants Skill: Level 20 Foo', '', 'Unknown words', '', 'Bonded: {0}']);
  const out = ['羁绊： +40 生命上限', undefined, lines[2], lines[3],
    undefined, '+40 生命上限', undefined, '获得技能：20 级 Foo', undefined, 'Unknown words', undefined, '羁绊： {0}'];
  expect(localizedLines(lines, 'zh-CN', out))
    .toEqual(['纽带：+40 生命上限', '物品等级: 83', '纽带：获得技能：20 级 Foo', 'Bonded: Unknown words']);
  // CN uses the official prefix even when a pack has no prefix template.
  expect(localizedLines([lines[2]], 'zh-CN', [lines[2], undefined, '获得技能：20 级 Foo', undefined, 'Bonded: {0}']))
    .toEqual(['纽带：获得技能：20 级 Foo']);
  // A whole-line row that kept the English prefix gives way to the pack's own prefix around the effect…
  expect(localizedLines([lines[0]], 'zh-CN', ['Bonded: +40 生命上限', undefined, '+40 生命上限', undefined, '纽带：{0}']))
    .toEqual(['纽带：+40 生命上限']);
  // …and is still shown when the pack cannot compose one (a partly translated line beats English).
  expect(localizedLines([lines[0]], 'zh-TW', ['Bonded: 生命上限 +40'])).toEqual(['Bonded: 生命上限 +40']);
  expect(localizedLines([lines[0]], 'zh-CN', [])).toEqual([lines[0]]);
  expect(linesToTranslate(['+5 to Strength'], 'zh-CN')).toEqual(['+5 to Strength']);
});

it('merges a wrapped Bonded effect before whole-line and fallback translation, preserving row positions', () => {
  const head = 'Bonded: 12% chance when collecting an Elemental Infusion to gain an';
  const tail = 'additional Elemental Infusion of the same type';
  const lines = [head, tail, 'Bonded: Archon recovery period expires 30% faster', '+40 to maximum Life'];
  const sent = linesToTranslate(lines, 'zh-CN');
  expect(sent[0]).toBe(`${head} ${tail}`);
  expect(sent[1]).toBe('');
  expect(sent[5]).toBe(`${head.replace('Bonded: ', '')} ${tail}`);
  const cn = localizedLines(lines, 'zh-CN', ['羁绊：收集元素灌注时有 12% 的几率额外获得一个同类元素灌注', undefined, '纽带：执政官恢复期的结束速度加快 30%', '+40 生命上限']);
  expect(cn).toEqual(['纽带：收集元素灌注时有 12% 的几率额外获得一个同类元素灌注', '', '纽带：执政官恢复期的结束速度加快 30%', '+40 生命上限']);
  expect(localizedLines(lines, 'en-US', [])[0]).toBe(`${head} ${tail}`);
  expect(localizedLines([lines[2]], 'zh-TW', ['連結：恢復期加快 30%'])).toEqual(['連結：恢復期加快 30%']);
  expect(lines).toEqual([head, tail, lines[2], '+40 to maximum Life']);
  expect(linesToTranslate(['Bonded: +40 to maximum Life', '+50 to maximum Mana'], 'zh-CN').slice(0, 2))
    .toEqual(['Bonded: +40 to maximum Life', '+50 to maximum Mana']);
});
