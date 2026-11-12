import { expect, test } from 'vitest';
import type { PassiveJewelState, PassiveNode, TreeEditResult } from '../../api/types';
import { jewelSocketNames } from './jewelSocketNames';
import { passiveNameKey } from './passiveNodeNames';
import { defaultReportStat, defaultTravelAttribute, distributeAttributeChoices, passivePointSummary, passivePointSummaryTarget, powerGain, treeHoverLines } from './treePresentation';

const node = (skill: number, patch: Partial<PassiveNode> = {}): PassiveNode => ({ skill, id: String(skill), kind: 'normal', ...patch });

test.each([
  ['en-US', 'Strength', 'Dexterity', 'Intelligence'],
  ['zh-CN', '力量', '敏捷', '智慧'],
  ['zh-TW', '力量', '敏捷', '智慧'],
] as const)('hover lines follow attribute changes without retranslating the node in %s', (lang, str, dex, int) => {
  const stats = ['+5'];
  expect(treeHoverLines(stats, 'str', lang)).toEqual(['+5', `→ ${str}`]);
  expect(treeHoverLines(stats, 'dex', lang)).toEqual(['+5', `→ ${dex}`]);
  expect(treeHoverLines(stats, 'int', lang)).toEqual(['+5', `→ ${int}`]);
  expect(treeHoverLines(stats, undefined, lang)).toEqual(['+5']);
  expect(stats).toEqual(['+5']);
});

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

test('ordinary points exclude data-defined starts and read the budget from the engine', () => {
  const nodes = [node(1, { name: 'NEW START' }), node(2), node(3, { kind: 'ascendancy_start', ascendancy_id: 'a' }),
    node(4, { ascendancy_id: 'a', stats: ['Grants 2 Passive Skill Points'] }), node(5), node(6), node(7)];
  const swap = { active: 1 as const, alternate_items: [], exclusive_nodes: [[5], [6, 7]] as [number[], number[]] };
  const effects: PassiveJewelState = { class_starts: { wayfarer: 1 }, allocation_grants: [],
    nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] };
  const current: TreeEditResult = { points_used: 3, points_max: 131, weapon_points_used: [1, 2], weapon_points_max: 32,
    added: [], removed: [], promoted: [], path: [], allocation: { shared: [1, 2, 3, 4], exclusive: [[5], [6, 7]] } };
  expect(passivePointSummary(nodes, [1, 2, 3, 4, 5], swap, effects, current)).toEqual({ used: 3, max: 131, weapon: [1, 2] });
  expect(passivePointSummary(nodes, [1, 2, 3, 4, 6, 7], { ...swap, active: 2 }, effects)).toEqual({ used: 3, max: null, weapon: [1, 2] });
  expect(passivePointSummary(nodes, [1, 999], undefined, effects).used).toBeNull();
  expect(passivePointSummary(nodes, [1, 2]).used).toBeNull();
  expect(passivePointSummary(nodes, [])).toEqual({ used: 0, max: null, weapon: [0, 0] });
  expect(passivePointSummary(nodes, [], undefined, effects, { ...current, added: [2] }).max).toBeNull();
});

test('an empty tree reads its engine budget from a weapon-only neighbor preview without spending a point', () => {
  const nodes = [node(1, { name: 'NEW START' }), node(2, { stats: ['Grants 9 Passive Skill Points'] })];
  const starts = new Set([1]);
  expect(passivePointSummaryTarget(nodes, new Set(), starts, [2])).toEqual({ node: 2, mode: 1 });
  const probe: TreeEditResult = { points_used: 1, points_max: 113, weapon_points_used: [1, 0], weapon_points_max: 24,
    added: [2], removed: [], promoted: [], path: [1, 2], allocation: { shared: [], exclusive: [[2], []] } };
  expect(passivePointSummary(nodes, [], undefined, undefined, probe)).toEqual({ used: 0, max: 113, weapon: [0, 0] });
  // A shared allocation can grant ordinary points, so its budget cannot stand in for an empty tree.
  expect(passivePointSummary(nodes, [], undefined, undefined, {
    ...probe, points_max: 122, allocation: { shared: [2], exclusive: [[], []] },
  }).max).toBeNull();
  expect(passivePointSummaryTarget(nodes, new Set(), starts, [])).toEqual({ node: null, mode: 1 });
  expect(passivePointSummaryTarget(nodes, new Set([2]), starts, [2])).toEqual({ node: 2, mode: 0 });
  expect(passivePointSummaryTarget(nodes, new Set([2]), starts, [2], {
    active: 2, alternate_items: [], exclusive_nodes: [[], [2]],
  })).toEqual({ node: 2, mode: 2 });
  expect(passivePointSummaryTarget(nodes, new Set(), starts, [2], {
    active: 1, alternate_items: [], exclusive_nodes: [[], [2]],
  })).toEqual({ node: 2, mode: 2 });
});

test('a tree containing only implicit starts still probes a regular neighbor for its budget', () => {
  const nodes = [node(1), node(2), node(3, { kind: 'ascendancy_start', ascendancy_id: 'future' })];
  expect(passivePointSummaryTarget(nodes, new Set([1, 3]), new Set([1]), [2])).toEqual({ node: 2, mode: 1 });
  const probe: TreeEditResult = { points_used: 1, points_max: 113, weapon_points_used: [1, 0], weapon_points_max: 24,
    added: [2], removed: [], promoted: [], path: [1, 2], allocation: { shared: [1, 3], exclusive: [[2], []] } };
  const effects: PassiveJewelState = { class_starts: { wayfarer: 1 }, allocation_grants: [],
    nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] };
  expect(passivePointSummary(nodes, [1, 3], undefined, effects, probe)).toEqual({ used: 0, max: 113, weapon: [0, 0] });
});

test('travel defaults follow new class base attributes and retain the majority of saved choices', () => {
  const characterClass = { name: 'Wayfarer', base_str: 30, base_dex: 7, base_int: 7 };
  expect(defaultTravelAttribute({}, characterClass)).toBe('str');
  expect(defaultTravelAttribute({}, { ...characterClass, base_int: 40 })).toBe('int');
  expect(defaultTravelAttribute({}, { ...characterClass, base_dex: 40 })).toBe('dex');
  expect(defaultTravelAttribute({}, { ...characterClass, base_str: 11, base_int: 11 })).toBe('int');
  expect(defaultTravelAttribute({ '1': 'int', '2': 'int', '3': 'str' }, characterClass)).toBe('int');
  expect(defaultTravelAttribute({})).toBe('dex');
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


test('node report metric follows life, survival and damage goals with an available fallback', () => {
  const stats = ['TotalDPS', 'Life', 'TotalEHP'];
  expect(defaultReportStat('life', stats)).toBe('Life');
  expect(defaultReportStat('ehp', stats)).toBe('TotalEHP');
  expect(defaultReportStat('dps', stats)).toBe('TotalDPS');
  expect(defaultReportStat('balanced', stats)).toBe('TotalDPS');
  expect(defaultReportStat('dps', ['Life'])).toBe('Life');
  // A summoner build measures its minion for a damage goal, and keeps other goals.
  const summoner = ['MainMinionDPS', ...stats];
  expect(defaultReportStat('dps', summoner)).toBe('MainMinionDPS');
  expect(defaultReportStat('balanced', summoner)).toBe('MainMinionDPS');
  expect(defaultReportStat('life', summoner)).toBe('Life');
});
