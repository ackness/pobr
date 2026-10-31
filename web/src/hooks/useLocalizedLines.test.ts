import { expect, it } from 'vitest';
import { localizeStructLine } from './useLocalizedLines';
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
