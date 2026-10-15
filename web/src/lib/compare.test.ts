import { describe, expect, test } from 'vitest';
import { diffStats } from './compare';
import type { DisplayStatValue } from '../api/types';

const stats = (pairs: [string, number | null][]): DisplayStatValue[] =>
  pairs.map(([id, value]) => ({ id, value, category: 'Offence' }));

describe('diffStats', () => {
  test('返回非零差异并按幅度排序', () => {
    const before = stats([
      ['TotalDPS', 100],
      ['Life', 50],
      ['Mana', 40],
    ]);
    const after = stats([
      ['TotalDPS', 150],
      ['Life', 45],
      ['Mana', 40],
    ]);
    const diffs = diffStats(before, after);
    expect(diffs.map((d) => d.id)).toEqual(['TotalDPS', 'Life']);
    expect(diffs[0].delta).toBe(50);
    expect(diffs[1].delta).toBe(-5);
  });

  test('忽略 null（∞）字段与浮点噪声', () => {
    const before = stats([
      ['ChaosMaxHit', null],
      ['Life', 50],
    ]);
    const after = stats([
      ['ChaosMaxHit', 999],
      ['Life', 50.001],
    ]);
    expect(diffStats(before, after)).toEqual([]);
  });
});

import { allocationDiff, compareStats, comparisonStats, comparisonRequests, configDifferences, copySection, diffText, formatComparisonDelta, itemDifferences, resolvedTreeVersion, skillDifferences, weaponDifferences, type StageState } from './compare';
import { switchWeapons } from './weaponSets';

const state = (): StageState => ({
  pobCode: null, treeVersion: '0_5', character: { class_name: 'Ranger', ascendancy_name: '', level: 20 },
  allocatedNodes: [1, 2], attributeChoices: { '2': 'dex' }, items: [{ slot: 'ring1', text: 'Original' }],
  flasks: [], limbs: [], jewels: [], socketGroups: [{ enabled: true, gems: [{ skill_id: 'SparkPlayer', level: 5, quality: 0 }] }],
  annotations: { 'item:ring1': 'Keep this note' }, params: { main_socket_group: 0, config_inputs: { falseFlag: false, zero: 0 } },
});

describe('whole-build comparison', () => {
  test('retains missing, null and non-finite values and never divides by zero', () => {
    const rows = compareStats(stats([['Life', 0], ['Mana', 10], ['ChaosMaxHit', null], ['Infinite', Infinity]]),
      stats([['Life', 50], ['ChaosMaxHit', 20], ['Infinite', Infinity], ['New', 5]]));
    expect(rows.find(row => row.id === 'Life')).toMatchObject({ delta: 50, percent: null, changed: true });
    expect(rows.find(row => row.id === 'Mana')).toMatchObject({ after: undefined, delta: null, changed: true });
    expect(rows.find(row => row.id === 'ChaosMaxHit')).toMatchObject({ before: null, delta: null, changed: true });
    expect(rows.find(row => row.id === 'Infinite')).toMatchObject({ delta: null, changed: false });
    expect(rows.find(row => row.id === 'New')).toMatchObject({ before: undefined, delta: null });
    expect(compareStats(stats([['Life', 50]]), stats([['Life', 40]]))[0]).toMatchObject({ delta: -10, percent: -20 });
  });
  test('shares only A config, clearing B overrides without replacing its selected skill', () => {
    const a = { main_socket_group: 0, config_inputs: { zero: 0, flag: false }, enemy_tier: 'boss' as const };
    const b = { main_socket_group: 2, config_inputs: { other: true }, enemy_tier: 'uber' as const, extra_modifiers: ['+20 to maximum Life'] };
    expect(comparisonRequests(a, b, 'own')[1]).toMatchObject(b);
    const [aa, bb] = comparisonRequests(a, b, 'a');
    expect(bb).toMatchObject({ main_socket_group: 2, config_inputs: a.config_inputs, enemy_tier: 'boss', mode_effective: true, config_context: false });
    expect(bb.extra_modifiers).toBeUndefined();
    expect(aa.main_socket_group).toBe(0);
    expect(b.extra_modifiers).toEqual(['+20 to maximum Life']);
  });
  test('compares repeated modifier lines without dropping multiplicity or order', () => {
    expect(diffText('One\nOne\nTwo', 'One\nThree')).toEqual({ before: [
      { text: 'One', changed: false }, { text: 'One', changed: true }, { text: 'Two', changed: true },
    ], after: [{ text: 'One', changed: false }, { text: 'Three', changed: true }] });
  });
  test('copies one raw item, preserving all other A fields and B', () => {
    const a = state(), b = { ...state(), items: [{ slot: 'ring1', text: 'Candidate\n+100 to maximum Life' }] };
    const original = structuredClone([a, b]);
    expect(itemDifferences(a, b)).toEqual([{ section: 'items', slot: 'ring1', before: 'Original', after: b.items[0].text }]);
    const next = copySection(a, b, { kind: 'item', section: 'items', slot: 'ring1' });
    expect(next).toEqual({ ...a, items: b.items });
    expect(next.items[0]).not.toBe(b.items[0]);
    expect([a, b]).toEqual(original);
    expect(copySection(a, { ...b, items: [] }, { kind: 'item', section: 'items', slot: 'ring1' }).items).toEqual([]);
  });
  test('compares and copies weapons by their actual set, preserving active selection and nodes', () => {
    const a: StageState = { ...state(), items: [{ slot: 'weapon1', text: 'First' }],
      weaponSwap: { active: 1, alternate_items: [{ slot: 'weapon1', text: 'Second' }], exclusive_nodes: [[2], [3]] } };
    const b = switchWeapons({ ...a, weaponSwap: { ...a.weaponSwap!, alternate_items: [{ slot: 'weapon1', text: 'Upgrade' }] } }, 2);
    expect(itemDifferences(a, b)).toEqual([]);
    expect(weaponDifferences(a, b)).toEqual([{ set: 2, slot: 'weapon1', before: 'Second', after: 'Upgrade' }]);
    const next = copySection(a, b, { kind: 'weapon', set: 2, slot: 'weapon1' });
    expect(next.items).toEqual(a.items);
    expect(next.allocatedNodes).toEqual(a.allocatedNodes);
    expect(next.weaponSwap).toEqual({ ...a.weaponSwap, alternate_items: b.items });
  });
  test('keeps missing, false and zero separate while copying config', () => {
    const a = state(), b = { ...state(), params: { config_inputs: { added: false, zero: 0 } } };
    expect(configDifferences(a, b)).toEqual([{ key: 'added', before: undefined, after: false }, { key: 'falseFlag', before: false, after: undefined }]);
    expect(copySection(a, b, { kind: 'config', key: 'added' }).params.config_inputs.added).toBe(false);
    expect(copySection(a, b, { kind: 'config', key: 'zero' }).params.config_inputs.zero).toBe(0);
    expect(copySection(a, b, { kind: 'config', key: 'falseFlag' }).params.config_inputs).not.toHaveProperty('falseFlag');
    expect(copySection(a, b, { kind: 'config', key: 'added' }).params.main_socket_group).toBe(0);
  });
  test('copies skill level/quality/support/form/count and main selection as independent values', () => {
    const a = state(), b = state();
    b.socketGroups.push({ enabled: true, include_in_full_dps: true, group_count: 2, main_active_skill: 2,
      gems: [{ skill_id: 'FireballPlayer', level: 12, quality: 17, stat_set_index: 2, count: 3, enable_global_1: false },
        { skill_id: 'SupportConcentratedEffectPlayer', level: 1, quality: 0 }] });
    b.params.main_socket_group = 1;
    expect(skillDifferences(a, b).map(row => row.index)).toEqual([0, 1]);
    const next = copySection(a, b, { kind: 'skill', index: 1 });
    expect(next.socketGroups[1]).toEqual(b.socketGroups[1]);
    expect(next.socketGroups[1].gems).not.toBe(b.socketGroups[1].gems);
    expect(next.params.main_socket_group).toBe(1);
    expect(a.socketGroups).toHaveLength(1);
  });
  test('removing a group remaps selection and notes; granted groups cannot be detached', () => {
    const a = state(), b = state();
    a.socketGroups.push({ enabled: true, gems: [] }); a.params.main_socket_group = 1;
    a.annotations['skill:0'] = 'First'; a.annotations['skill:1'] = 'Second';
    b.socketGroups = [];
    const next = copySection(a, b, { kind: 'skill', index: 0 });
    expect(next.params.main_socket_group).toBe(0);
    expect(next.annotations).toEqual({ 'item:ring1': 'Keep this note', 'skill:0': 'Second' });
    a.socketGroups[0].source = 'Weapon 1';
    expect(copySection(a, b, { kind: 'skill', index: 0 })).toBe(a);
  });
  test('lists passive set differences and resolves Default against current data', () => {
    expect(allocationDiff([3, 2, 2, 1], [2, 4])).toEqual({ added: [4], removed: [1, 3], shared: [2] });
    expect(resolvedTreeVersion('Default', '0.5.5.4.2')).toBe('0_5');
    expect(resolvedTreeVersion(undefined, '0.5.5.4.2')).toBe('0_5');
    expect(resolvedTreeVersion('0_4', '0.5.5.4.2')).toBe('0_4');
  });
});


test('whole-build summary keeps absent main-skill outputs unavailable', () => {
  const output = { stats: stats([['TotalDPS', 0], ['Life', 100]]), main_skill: null } as import('../api/types').CalculateBuildResponse;
  expect(comparisonStats(output)).toEqual(stats([['Life', 100]]));
  const withSkill = { ...output, main_skill: { hit_dps: 100, dot_dps: 20, combined_dps: 120 } } as import('../api/types').CalculateBuildResponse;
  expect(comparisonStats(withSkill)).toContainEqual({ id: 'MainSkillCombinedDPS', value: 120, category: 'Offence' });
});


test('sidebar multiplier formats use percentage-point deltas and pierce all-target sentinel stays unbounded', () => {
  const movement = compareStats(stats([['EffectiveMovementSpeedMod', 1]]), stats([['EffectiveMovementSpeedMod', 1.1]]))[0];
  expect(movement.percent).toBeNull();
  expect(formatComparisonDelta(movement.delta, 'modifier', 'en-US')).toBe('+10%');
  expect(formatComparisonDelta(-0.5, 'chanceMultiplier', 'en-US')).toBe('-50%');
  const chance = compareStats(stats([['CritBifurcates', 1.2]]), stats([['CritBifurcates', 1.3]]))[0];
  expect(chance.percent).toBeCloseTo(50);
  expect(compareStats(stats([['PierceCount', 1]]), stats([['PierceCount', 100]]))[0]).toMatchObject({ delta: null, percent: null, changed: true });
});

import { compareTreeAllocations, treeAllocationTotals } from './compare';
import type { PassiveNode } from '../api/types';

const passive = (skill: number, patch: Partial<PassiveNode> = {}): PassiveNode => ({ skill, id: String(skill), kind: 'normal', x: skill * 100, y: 0, ...patch });

describe('tree comparison', () => {
  test('classifies a union without duplicate allocations and preserves jewels on both sides, including inactive sockets', () => {
    const a = { ...state(), allocatedNodes: [1, 2, 2], jewels: [{ socket_node: 2, text: 'A jewel' }] };
    const b = { ...state(), allocatedNodes: [2, 3], jewels: [{ socket_node: 2, text: 'B jewel' }, { socket_node: 4, text: 'Inactive' }] };
    const snapshot = structuredClone([a, b]);
    const nodes = [passive(1), passive(2, { kind: 'jewel_socket' }), passive(3), passive(4, { kind: 'jewel_socket' })];
    const result = compareTreeAllocations(a, b, nodes, nodes, true);
    expect(result.nodes.map(entry => [entry.key, entry.side])).toEqual([['1', 'a'], ['2', 'both'], ['3', 'b'], ['4', 'none']]);
    expect(result.nodes[1]).toMatchObject({ jewelA: 'A jewel', jewelB: 'B jewel' });
    expect(result.nodes[3]).toMatchObject({ side: 'none', jewelB: 'Inactive' });
    expect([a, b]).toEqual(snapshot);
  });
  test('does not conflate matching numeric IDs or invent coordinates across tree versions', () => {
    const a = { ...state(), allocatedNodes: [1, 99] }, b = { ...state(), allocatedNodes: [1] };
    const current = passive(1, { name: 'Current' }), historical = passive(1, { name: 'Old', x: undefined, y: undefined });
    const result = compareTreeAllocations(a, b, [current], [historical], false);
    expect(result.nodes.map(entry => [entry.key, entry.side, entry.node.name])).toEqual([['a:1', 'a', 'Current'], ['b:1', 'b', 'Old']]);
    expect(result.nodes[1].node.x).toBeUndefined();
    expect(result.unplacedA).toEqual([99]);
    expect(result.unplacedB).toEqual([1]);
  });
  test('counts unique passive/ascendancy points without starts, separates saved choices and diagnoses unknown IDs', () => {
    const nodes = [passive(1, { name: 'RANGER' }), passive(2, { name: 'Attribute', stats: ['+5 to any Attribute'] }),
      passive(3, { ascendancy_id: 'Deadeye', kind: 'ascendancy_start' }), passive(4, { ascendancy_id: 'Deadeye' }),
      passive(5, { name: 'Attribute', stats: ['+5 to any Attribute'] }), passive(6, { kind: 'jewel_socket' })];
    const build = { ...state(), allocatedNodes: [1, 2, 2, 3, 4, 5, 6, 99], attributeChoices: { '2': 'int' as const, '100': 'str' as const } };
    expect(treeAllocationTotals(build, nodes)).toEqual({ passive: 3, ascendancy: 1, unknown: [99], attributes: { str: 0, dex: 0, int: 1, unassigned: 1 } });
  });
});

test('shared comparison configuration includes party effects without changing either saved request', () => {
  const party = { members: [{ id: 'support', name: 'Support', enabled: true, blocks: {}, warnings: [] }], exports: {} };
  const [own, shared] = comparisonRequests({ party }, { party: { members: [], exports: {} } }, 'a');
  expect(own.party).toEqual(party);
  expect(shared.party).toEqual(party);
  expect(comparisonRequests({ party }, {}, 'own')[1].party).toBeUndefined();
});
