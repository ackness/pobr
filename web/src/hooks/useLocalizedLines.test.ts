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
