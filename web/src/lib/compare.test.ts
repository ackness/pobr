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

import { compareStats, comparisonStats, comparisonRequests, configDifferences, copySection, diffText, formatComparisonDelta, itemDifferences, resolvedTreeVersion, skillDifferences, weaponDifferences, type StageState } from './compare';
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
  test('resolves Default tree version against current data', () => {
    expect(resolvedTreeVersion('Default', '0.5.5.4.2')).toBe('0_5');
    expect(resolvedTreeVersion(undefined, '0.5.5.4.2')).toBe('0_5');
    expect(resolvedTreeVersion('0_4', '0.5.5.4.2')).toBe('0_4');
  });
});


test('whole-build summary keeps absent main-skill outputs unavailable', () => {
  const output = { stats: stats([['CombinedDPS', 0], ['TotalDPS', 0], ['Life', 100]]), main_skill: null } as import('../api/types').CalculateBuildResponse;
  expect(comparisonStats(output)).toEqual(stats([['Life', 100]]));
  const withSkill = { ...output, main_skill: { hit_dps: 100, dot_dps: 20, combined_dps: 120 } } as import('../api/types').CalculateBuildResponse;
  expect(comparisonStats(withSkill)).toContainEqual({ id: 'MainSkillCombinedDPS', value: 120, category: 'Offence' });
  expect(comparisonStats(withSkill)).toContainEqual({ id: 'CombinedDPS', value: 0, category: 'Offence' });
  const difference = compareStats(comparisonStats(withSkill), comparisonStats(output));
  expect(difference.find(row => row.id === 'CombinedDPS')).toMatchObject({ after: undefined, delta: null, percent: null });
});


test('sidebar multiplier formats use percentage-point deltas and pierce all-target sentinel stays unbounded', () => {
  const movement = compareStats(stats([['EffectiveMovementSpeedMod', 1]]), stats([['EffectiveMovementSpeedMod', 1.1]]))[0];
  expect(movement.percent).toBeNull();
  expect(formatComparisonDelta(movement.delta, 'modifier', 'en-US')).toBe('+10 pp');
  expect(formatComparisonDelta(-0.5, 'chanceMultiplier', 'en-US')).toBe('-50 pp');
  const chance = compareStats(stats([['CritBifurcates', 1.2]]), stats([['CritBifurcates', 1.3]]))[0];
  expect(chance.percent).toBeNull();
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


import { nearestCompareStage, hasComparisonSource, comparisonGemForm } from './compare';
import type { GemCatalogEntry } from '../api/types';

test('default B stays within the same build and prefers the closest previous stage', () => {
  const entries = [{ buildId: 'unrelated', stageId: 'blank' }, ...['first', 'middle', 'last'].map(stageId => ({ buildId: 'current', stageId }))];
  expect(nearestCompareStage(entries, entries[3])).toBe(entries[2]);
  expect(nearestCompareStage(entries, entries[2])).toBe(entries[1]);
  expect(nearestCompareStage(entries, entries[1])).toBe(entries[2]);
  expect(nearestCompareStage(entries, entries[0])).toBeUndefined();
  expect(nearestCompareStage(entries, undefined)).toBeUndefined();
});

test('missing, unknown and failed outputs cannot be assumed to be zero', () => {
  const rows = compareStats(stats([['Flask1LifeRecovery', 0], ['Charm2Duration', 10], ['Unknown', 0], ['ChaosMaxHit', 0]]), stats([['ChaosMaxHit', null]]));
  expect(rows[0]).toMatchObject({ after: undefined, changed: true, delta: null });
  expect(rows[1]).toMatchObject({ after: undefined, changed: true, delta: null });
  expect(rows[2]).toMatchObject({ after: undefined, changed: true, delta: null });
  expect(rows[3]).toMatchObject({ after: null, changed: true, delta: null });
  expect(compareStats(stats([['Flask1LifeRecovery', 0]]), stats([['Flask1LifeRecovery', null]]))[0].changed).toBe(true);
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('percentage stats show point differences without relative percentages in %s', lang => {
  const row = compareStats(stats([['FireResist', -30]]), stats([['FireResist', 20]]))[0];
  expect(row).toMatchObject({ delta: 50, percent: null });
  expect(formatComparisonDelta(row.delta, 'percent', lang)).toBe(lang === 'en-US' ? '+50 pp' : lang === 'zh-TW' ? '+50 個百分點' : '+50 个百分点');
  expect(compareStats(stats([['FireResist', 0]]), stats([['FireResist', 0]]))[0]).toMatchObject({ delta: 0, percent: null, changed: false });
  expect(formatComparisonDelta(null, 'percent', lang)).toBe('—');
});

test('copy availability rejects absent equipment, weapons, skills and config without rejecting zero or false', () => {
  const b = { ...state(), items: [], socketGroups: [], params: { config_inputs: { zero: 0, flag: false } } };
  for (const section of [
    { kind: 'item', section: 'items', slot: 'ring1' }, { kind: 'weapon', set: 1, slot: 'weapon1' },
    { kind: 'skill', index: 0 }, { kind: 'config', key: 'missing' }, { kind: 'enemy' }, { kind: 'modifiers' },
  ] as const) expect(hasComparisonSource(b, section)).toBe(false);
  expect(hasComparisonSource(b, { kind: 'config', key: 'zero' })).toBe(true);
  expect(hasComparisonSource(b, { kind: 'config', key: 'flag' })).toBe(true);
  expect(hasComparisonSource(state(), { kind: 'item', section: 'items', slot: 'ring1' })).toBe(true);
});

test('gem comparisons hide unchanged default forms and name both sides of a form change', () => {
  const gem = { skill_id: 'SparkPlayer', level: 20, quality: 0 };
  const catalog = new Map([['SparkPlayer', { effect_forms: [{ skill_id: 'SparkPlayer', stat_sets: [
    { index: 1, label: 'Normal', id: 'normal', stage: null }, { index: 2, label: 'Empowered', id: 'empowered', stage: null },
  ] }] } as GemCatalogEntry]]);
  const alternate = { ...gem, stat_set_index: 2 };
  expect(comparisonGemForm(gem, { ...gem, level: 21, stat_set_index: 1 }, catalog, 'en-US')).toBeNull();
  expect(comparisonGemForm(gem, undefined, catalog, 'en-US')).toBeNull();
  expect(comparisonGemForm(gem, alternate, catalog, 'en-US')).toBe('Normal');
  expect(comparisonGemForm(alternate, gem, catalog, 'en-US')).toBe('Empowered');
  expect(comparisonGemForm(alternate, alternate, catalog, 'en-US')).toBe('Empowered');
  expect(comparisonGemForm(alternate, gem, new Map(), 'en-US')).toBe('Form 2');
});


test('form labels use the selected secondary active effect supplied by a gem', () => {
  const gem = { skill_id: 'MultiEffectPlayer', level: 20, quality: 0, stat_set_index: 2 };
  const catalog = new Map([[gem.skill_id, { effect_forms: [{ skill_id: 'SecondaryPlayer', stat_sets: [
    { index: 2, label: 'Secondary empowered form', id: 'secondary', stage: null },
  ] }] } as GemCatalogEntry]]);
  expect(comparisonGemForm(gem, undefined, catalog, 'en-US', 'SecondaryPlayer')).toBe('Secondary empowered form');
});


test.each([['en-US', 'Form'], ['zh-CN', '形态'], ['zh-TW', '形態']] as const)(
  'unlabeled form changes keep distinct form numbers in %s', (lang, label) => {
    const a = { skill_id: 'SparkPlayer', level: 20, quality: 0, stat_set_index: 2 };
    const b = { ...a, stat_set_index: 3 };
    const catalog = new Map([[a.skill_id, { effect_forms: [{ skill_id: a.skill_id, stat_sets: [
      { index: 2, label: null, id: 'second', stage: null }, { index: 3, label: null, id: 'third', stage: null },
    ] }] } as GemCatalogEntry]]);
    expect(comparisonGemForm(a, b, catalog, lang)).toBe(`${label} 2`);
    expect(comparisonGemForm(b, a, catalog, lang)).toBe(`${label} 3`);
    expect(comparisonGemForm(a, b, new Map(), lang)).toBe(`${label} 2`);
    expect(comparisonGemForm(b, a, new Map(), lang)).toBe(`${label} 3`);
  },
);
