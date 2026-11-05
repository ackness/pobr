import { expect, test } from 'vitest';
import type { PassiveNode } from '../../api/types';
import { jewelSocketNames } from './jewelSocketNames';
import { passiveNameKey } from './passiveNodeNames';
import { distributeAttributeChoices, passivePointSummary, powerGain } from './treePresentation';

const node = (skill: number, patch: Partial<PassiveNode> = {}): PassiveNode => ({ skill, id: String(skill), kind: 'normal', ...patch });

test('attribute redistribution changes the chosen nodes, preserves other choices and leaves excess nodes unassigned', () => {
  const original = { '10': 'int' as const, '20': 'int' as const, '30': 'dex' as const, '99': 'str' as const };
  const ids = [30, 10, 20];
  expect(distributeAttributeChoices(ids, original, { str: 2, dex: 1, int: 0 }))
    .toEqual({ '10': 'str', '20': 'str', '30': 'dex', '99': 'str' });
  expect(distributeAttributeChoices(ids, original, { str: 0, dex: 0, int: 1 }))
    .toEqual({ '10': 'int', '99': 'str' });
  expect(original['20']).toBe('int');
  expect(ids).toEqual([30, 10, 20]);
});

test('ordinary points exclude starts and ascendancy, include the larger weapon set and account for granted passives', () => {
  const nodes = [node(1, { name: 'WITCH' }), node(2), node(3, { kind: 'ascendancy_start', ascendancy_id: 'a' }),
    node(4, { ascendancy_id: 'a', stats: ['Grants 2 Passive Skill Points'] }), node(5), node(6), node(7)];
  const swap = { active: 1 as const, alternate_items: [], exclusive_nodes: [[5], [6, 7]] as [number[], number[]] };
  expect(passivePointSummary(nodes, [1, 2, 3, 4, 5], 98, swap)).toEqual({ used: 3, max: 123, weapon: [1, 2] });
  expect(passivePointSummary(nodes, [1, 2, 3, 4, 6, 7], 98, { ...swap, active: 2 }).used).toBe(3);
  expect(passivePointSummary(nodes, [1, 999], 98).used).toBeNull();
  expect(passivePointSummary(nodes, [], 1)).toEqual({ used: 0, max: 24, weapon: [0, 0] });
});

test('report gains retain signs, fractional per-point values and the selected stat unit', () => {
  expect(powerGain(2.5, 'Life', 'en-US')).toBe('+2.5 points');
  expect(powerGain(-12.5, 'TotalDPS', 'en-US')).toBe('-12.5 /s');
  expect(powerGain(1.5, 'CritChance', 'en-US')).toBe('+1.5 percentage points');
  expect(powerGain(0.02, 'CritBifurcates', 'en-US')).toBe('+2 percentage points');
  expect(powerGain(0, 'Life', 'zh-CN')).toBe('+0 点');
  expect(powerGain(null, 'Life', 'en-US')).toBe('—');
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('socket labels use localized landmarks, disambiguate neighbors and fall back to ordinals in %s', lang => {
  const landmark = node(50, { name: '[Power|Power]', kind: 'notable', x: 0, y: 0 });
  const nodes = [node(7960, { kind: 'jewel_socket', x: 100, y: 0 }), node(10, { kind: 'jewel_socket' }),
    node(9000, { kind: 'jewel_socket', x: 50, y: 0 }), landmark];
  const names = new Map([[passiveNameKey(landmark), lang === 'en-US' ? '[Power|Power]' : lang === 'zh-CN' ? '威力' : '威能']]);
  const labels = jewelSocketNames(nodes, names, lang);
  expect(labels.get(10)).toMatch(/1$/);
  expect(labels.get(7960)).toContain(lang === 'en-US' ? 'Near Power' : lang === 'zh-CN' ? '威力' : '威能');
  expect(labels.get(7960)).not.toMatch(/7960|\[|\]/);
  expect(new Set(labels.values()).size).toBe(3);
  expect(jewelSocketNames([...nodes].reverse(), names, lang)).toEqual(labels);
});
