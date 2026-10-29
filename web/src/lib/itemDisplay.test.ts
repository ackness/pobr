import { expect, it } from 'vitest';
import type { ItemLineJson } from '../api/types';
import { itemDamageType, itemDisplaySections, itemValueParts } from './itemDisplay';

it('uses only supplied affix identities, keeps multiline affixes and separates sources', () => {
  const lines: ItemLineJson[] = [
    { kind: 'name', text: 'Woe Rain' }, { kind: 'base', text: 'Seeker Bow' },
    { kind: 'struct', text: 'Unique ID: hidden-123' }, { kind: 'struct', text: '--------' },
    { kind: 'struct', text: 'Implicits: 2' }, { kind: 'struct', text: 'Requirements:' },
    { kind: 'struct', text: 'Item Level: 81' }, { kind: 'struct', text: 'Corrupted' },
    { kind: 'struct', text: 'Rune: Perfect Iron Rune' },
    { kind: 'struct', text: 'Note: keep this note' },
    { kind: 'enchant', text: '23% increased Elemental Damage with Attacks' },
    { kind: 'implicit', text: '+30% to Cold Resistance' },
    { kind: 'rune', text: 'Bow Attacks fire an additional Arrow' },
    { kind: 'explicit', text: 'Adds 12 to 228 Lightning Damage', affix: 'prefix', tier: 1 },
    { kind: 'explicit', text: 'Hybrid second line', affix: 'prefix', tier: 1 },
    { kind: 'explicit', text: '14% increased Attack Speed', affix: 'suffix', tier: 2 },
    { kind: 'explicit', text: '+4.92% to Critical Hit Chance' },
    { kind: 'explicit', text: 'Unknown effect: retain <all> & text' },
  ];
  const translated = lines.map(line => line.text === 'Item Level: 81' ? '物品等级: 81' : line.text);
  const result = itemDisplaySections(lines, translated);
  expect(result.properties).toEqual([{ label: '物品等级', value: '81', index: 6 }]);
  expect(result.prefixes.map(line => line.text)).toEqual(['Adds 12 to 228 Lightning Damage', 'Hybrid second line']);
  expect(result.suffixes).toHaveLength(1);
  expect(result.other.map(line => line.text)).toEqual(['+4.92% to Critical Hit Chance', 'Unknown effect: retain <all> & text']);
  expect(result.effects.map(line => line.kind)).toEqual(['enchant', 'implicit', 'rune']);
  expect(result.metadata.map(line => line.text)).toEqual(['Note: keep this note']);
  expect(result.runes[0].text).toBe('Rune: Perfect Iron Rune');
  expect(result.states[0].text).toBe('Corrupted');
  expect(result.explicit.map(line => line.index)).toEqual([13, 14, 15, 16, 17]);
});

it.each([
  ['Adds 12 - 228 Lightning Damage', ['12', '228']],
  ['暴击几率 +4.92%，减少 -10%，每 1,234 点获得 +2', ['+4.92%', '-10%', '1,234', '+2']],
  ['(12–15) to (228–250)', ['12', '15', '228', '250']],
  ['Corrupted', []],
])('highlights values without changing %s', (text, values) => {
  const parts = itemValueParts(text);
  expect(parts.map(part => part.text).join('')).toBe(text);
  expect(parts.filter(part => part.numeric).map(part => part.text)).toEqual(values);
});

it('does not colour mixed damage or infer affixes from translated text', () => {
  expect(itemDamageType('Adds 69 to 85 Cold Damage')).toBe('cold');
  expect(itemDamageType('Gain Fire Damage as Extra Cold Damage')).toBeUndefined();
  const result = itemDisplaySections([{ kind: 'explicit', text: 'Unknown modifier' }], ['附加 12 点闪电伤害']);
  expect(result.other[0]).toMatchObject({ original: 'Unknown modifier', text: '附加 12 点闪电伤害' });
  expect(result.prefixes).toHaveLength(0);
});

it('formats known structural units while retaining unfamiliar socket formats and metadata', () => {
  const fields = ['LevelReq: 77', 'Dex: 163', 'Quality: 20', 'Sockets: S S', 'Sockets: R-G B', 'Armour: 0'];
  const model = itemDisplaySections(fields.map(text => ({ kind: 'struct', text })), fields);
  expect(model.properties.map(({ label, value }) => [label, value])).toEqual([
    ['Required level', '77'], ['Required Dexterity', '163'], ['Quality', '+20%'], ['Sockets', '2'], ['Sockets', 'R-G B'], ['Armour', '0'],
  ]);
});
