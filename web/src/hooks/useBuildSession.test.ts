import { beforeEach, describe, expect, test, vi } from 'vitest';
import { applyNamedSkills, applyNamedConfig, toRequest, backfillFromDecoded, materialize, paramsFromDecoded, parseSaved, prepareTreePlan, skillMetricsForState, type SavedSession } from './useBuildSession';
import type { BuildJson, CalculateBuildRequest, SkillMetricsResponse } from '../api/types';
import { parseWorkspace, createWorkspace } from '../lib/buildWorkspace';

// The engine answers a skill overview request with a row for each group the
// request enables, named after the weapon set the request names.
const engine = vi.hoisted(() => ({ requests: [] as CalculateBuildRequest[] }));
vi.mock('../api/backend', () => ({ getBackend: async () => ({
  skillMetrics: async (request: CalculateBuildRequest): Promise<SkillMetricsResponse> => {
    engine.requests.push(request);
    return { item_errors: [], skills: (request.socket_groups ?? []).flatMap((group, group_index) => group.enabled ? [{
      group_index, active_skill_index: 1, skill_id: `${group.gems[0].skill_id} in set ${request.weapon_swap?.active}`,
      status: 'calculated' as const, unsupported_modifiers: [], error: null,
      metrics: { hit_dps: 0, dot_dps: 0, combined_dps: 0, average_hit: 0, action_rate: 0, mana_cost: 0, life_cost: 0, cooldown: 0 },
    }] : []) };
  },
}) }));

const saved: SavedSession = {
  version: 1, notes: 'Keep this build', state: {
    character: { class_name: 'Witch', ascendancy_name: '', level: 1 }, pobCode: null,
    treeVersion: '0_1', allocatedNodes: [770], attributeChoices: { '770': 'int' },
    socketGroups: [{ enabled: true, gems: [{ skill_id: 'IceNovaPlayer', level: 20, quality: 0, stat_set_index: 2 }] }],
    items: [{ slot: 'ring1', text: 'Rarity: NORMAL\nSapphire Ring' }], flasks: [],
    limbs: [{ slot: 'arm1', text: 'Rarity: NORMAL\nCombat Arm\nImplicits: 1\n6% increased Attack Speed' }], jewels: [],
    annotations: {}, params: { config_inputs: { conditionFullLife: false, multiplier: 0 } },
  },
};

const parseState = (patch: Record<string, unknown>) => parseSaved(JSON.stringify({ ...saved, state: { ...saved.state, ...patch } }));

describe('complete jewel/passive plan application', () => {
  const current: SavedSession['state'] = {
    ...saved.state,
    allocatedNodes: [770, 99], attributeChoices: { '770': 'int', '99': 'str' },
    jewels: [{ socket_node: 99, text: 'Old Jewel' }],
    weaponSwap: { active: 1, alternate_items: [{ slot: 'weapon1', text: 'Other Weapon' }], exclusive_nodes: [[], []] },
  };
  const expected: CalculateBuildRequest = {
    tree_version: current.treeVersion, character: current.character,
    allocated_nodes: current.allocatedNodes, attribute_choices: current.attributeChoices,
    socket_groups: current.socketGroups, weapon_swap: current.weaponSwap, items: current.items, flasks: current.flasks,
    limbs: current.limbs, jewels: current.jewels, main_socket_group: current.params.main_socket_group,
    enemy_tier: current.params.enemy_tier, extra_modifiers: current.params.extra_modifiers,
    custom_modifier_blocks: current.params.custom_modifier_blocks,
    config_inputs: current.params.config_inputs,
  };
  const plan = {
    jewels: [{ socket_node: 10, text: 'New Jewel' }],
    allocatedNodes: [770, 10, 12], attributeChoices: { '12': 'dex' as const },
  };

  test('updates all three tree fields together and preserves unrelated session state', () => {
    const next = prepareTreePlan(current, expected, plan, false);
    expect(next).toMatchObject({
      jewels: plan.jewels, allocatedNodes: plan.allocatedNodes,
      attributeChoices: { '770': 'int', '12': 'dex' },
    });
    expect(next?.attributeChoices).not.toHaveProperty('99');
    for (const key of ['character', 'socketGroups', 'items', 'flasks', 'limbs', 'weaponSwap', 'params', 'annotations', 'pobCode'] as const) {
      expect(next?.[key]).toBe(current[key]);
    }
    expect(current.jewels).toEqual([{ socket_node: 99, text: 'Old Jewel' }]);
  });

  test('rejects stale, busy, weapon-exclusive and malformed plans without changing state', () => {
    expect(prepareTreePlan(current, { ...expected, items: [] }, plan, false)).toBeNull();
    expect(prepareTreePlan(current, expected, plan, true)).toBeNull();
    expect(prepareTreePlan({ ...current, weaponSwap: { ...current.weaponSwap!, exclusive_nodes: [[99], []] } },
      expected, plan, false)).toBeNull();
    expect(prepareTreePlan(current, expected, { ...plan, allocatedNodes: [770, 770] }, false)).toBeNull();
    expect(prepareTreePlan(current, expected, { ...plan, jewels: [plan.jewels[0], plan.jewels[0]] }, false)).toBeNull();
    expect(prepareTreePlan(current, expected, { ...plan, attributeChoices: { '12': 'invalid' } as never }, false)).toBeNull();
    expect(current.allocatedNodes).toEqual([770, 99]);
  });
});

describe('skill overview across weapon sets', () => {
  const gem = (skill_id: string) => ({ skill_id, level: 20, quality: 0 });
  const state: SavedSession['state'] = {
    ...saved.state,
    items: [{ slot: 'weapon1', text: 'Bow' }, { slot: 'ring1', text: 'Ring' }],
    socketGroups: [
      { enabled: true, gems: [gem('Shared')] },
      { enabled: true, weapon_set: 2, gems: [gem('Second')] },
      { enabled: true, weapon_set: 1, gems: [gem('First')] },
    ],
    weaponSwap: { active: 1, alternate_items: [{ slot: 'weapon1', text: 'Staff' }], exclusive_nodes: [[], []] },
  };
  beforeEach(() => { engine.requests.length = 0; });

  test('each set is calculated by a request that names it, with that set\'s weapons and groups', async () => {
    const report = await skillMetricsForState(state);
    expect(engine.requests.map(request => request.weapon_swap?.active)).toEqual([1, 2]);
    expect(engine.requests.map(request => request.items?.find(item => item.slot === 'weapon1')?.text)).toEqual(['Bow', 'Staff']);
    expect(engine.requests.map(request => request.socket_groups?.map(group => group.enabled)))
      .toEqual([[true, false, true], [true, true, false]]);
    // The group bound to the set not in use is calculated in that set; a shared
    // group only in the set in use.
    expect(report.skills.map(row => row.skill_id)).toEqual(['Shared in set 1', 'Second in set 2', 'First in set 1']);
  });

  test('a build that never swapped weapons names the first set', async () => {
    await skillMetricsForState({ ...state, socketGroups: [state.socketGroups[0]], weaponSwap: undefined });
    expect(engine.requests.map(request => request.weapon_swap))
      .toEqual([{ active: 1, alternate_items: [], exclusive_nodes: [[], []] }]);
  });
});

describe('saved-session validation', () => {
  test('restores PoB enemy tier and custom modifier controls from config', () => {
    const decoded = { config_inputs: {
      enemyIsBoss: 'None', customMods: '+100 to maximum Life\n+20 to Spirit',
      conditionFullLife: false,
    } } as unknown as BuildJson;
    expect(paramsFromDecoded(decoded)).toEqual({
      enemy_tier: 'none',
      extra_modifiers: ['+100 to maximum Life', '+20 to Spirit'],
      config_inputs: { conditionFullLife: false },
    });
  });

  test('keeps decoded native blocks in source order with raw text and disabled state', () => {
    const blocks = [
      { title: ' Life & Spirit ', enabled: true, text: '\n+100 to maximum Life\n\n+20 to Spirit\n' },
      { title: 'Later', enabled: false, text: '+500 to maximum Life\r\n' },
    ];
    const decoded = { config_inputs: {
      customMods: '+100 to maximum Life\n+500 to maximum Life',
      conditionFullLife: false,
    }, custom_modifier_blocks: blocks } as unknown as BuildJson;
    expect(paramsFromDecoded(decoded)).toEqual({
      config_inputs: { conditionFullLife: false },
      custom_modifier_blocks: blocks,
    });
  });

  test('migrates saved legacy config keys into dedicated controls', () => {
    const restored = parseState({ params: { config_inputs: {
      enemyIsBoss: 'Boss', customMods: '+50 to maximum Life', conditionFullLife: false,
    } } });
    expect(restored?.state.params).toEqual({
      config_inputs: { conditionFullLife: false },
      enemy_tier: 'boss',
      extra_modifiers: ['+50 to maximum Life'],
    });
  });

  test('keeps both legacy and dedicated custom modifier lanes in saved requests', () => {
    const restored = parseState({ params: {
      config_inputs: { enemyIsBoss: 'Boss', customMods: '+50 to maximum Life' },
      enemy_tier: 'none', extra_modifiers: ['+50 to maximum Life', '+20 to Spirit'],
    } });
    expect(restored?.state.params).toEqual({
      config_inputs: {}, enemy_tier: 'boss',
      extra_modifiers: ['+50 to maximum Life', '+50 to maximum Life', '+20 to Spirit'],
    });
  });

  test('grouped saved modifiers replace both legacy calculation lanes without altering raw blocks', () => {
    const blocks = [
      { title: 'Active', enabled: true, text: ' +50 to maximum Life\n\n+20 to Spirit ' },
      { title: 'Paused', enabled: false, text: '+500 to maximum Life' },
    ];
    const restored = parseState({ params: {
      config_inputs: { customMods: '+50 to maximum Life', conditionFullLife: false },
      extra_modifiers: ['+50 to maximum Life'],
      custom_modifier_blocks: blocks,
    } });
    expect(restored?.state.params).toEqual({
      config_inputs: { conditionFullLife: false }, custom_modifier_blocks: blocks,
    });
    expect(restored?.state.params.custom_modifier_blocks).toEqual(blocks);
  });

  test('explicit empty grouped list clears legacy saved modifiers', () => {
    const restored = parseState({ params: {
      config_inputs: { customMods: '+50 to maximum Life' },
      extra_modifiers: ['+100 to maximum Life'],
      custom_modifier_blocks: [],
    } });
    expect(restored?.state.params).toEqual({ config_inputs: {}, custom_modifier_blocks: [] });
    expect(paramsFromDecoded({ config_inputs: { customMods: '+300 to maximum Life' },
      custom_modifier_blocks: [] } as unknown as BuildJson)).toEqual({
      config_inputs: {}, custom_modifier_blocks: [],
    });
  });

  test('preserves imported skill forms, historical trees and explicit false/zero settings', () => {
    expect(parseSaved(JSON.stringify(saved))).toEqual(saved);
  });

  test('normalizes omitted legacy fields and partial params before they reach rendering', () => {
    const restored = parseState({ treeVersion: undefined, params: {}, attributeChoices: undefined,
      flasks: undefined, limbs: undefined, jewels: undefined, annotations: undefined });
    expect(restored?.state.params.config_inputs).toEqual({});
    expect(Object.keys(restored!.state.params.config_inputs)).toEqual([]);
    expect(restored?.state.attributeChoices).toEqual({});
    expect(restored?.state.flasks).toEqual([]);
    expect(restored?.state.limbs).toEqual([]);
    expect(restored?.state.jewels).toEqual([]);
  });

  test('materializes the decoded limbs as a set of their own', () => {
    const limb = { slot: 'leg2', text: 'Rarity: NORMAL\nEvasive Leg\nImplicits: 1\n25% increased Evasion Rating' };
    const items = { equipped: [], socket_jewels: [], flasks: [] };
    const decoded = (limbs?: unknown[]) => ({
      config_inputs: {}, tree: {}, socket_groups: [], items: limbs ? { ...items, limbs } : items,
    }) as unknown as BuildJson;
    expect(materialize(decoded([limb])).limbs).toEqual([limb]);
    expect(materialize(decoded([limb])).items).toEqual([]);
    expect(materialize(decoded()).limbs).toEqual([]);
  });

  test.each([
    { character: { class_name: 'Witch', level: '80' } },
    { character: { class_name: 'Witch', level: -1 } },
    { character: { class_name: 'Witch', level: 101 } },
    { allocatedNodes: ['770'] },
    { allocatedNodes: [-1] },
    { socketGroups: [null] },
    { socketGroups: [{ enabled: true, gems: [null] }] },
    { socketGroups: [{ enabled: true, gems: [{ skill_id: 'IceNovaPlayer', level: 20, quality: -1 }] }] },
    { socketGroups: [{ enabled: true, gems: [{ skill_id: 'IceNovaPlayer', level: 20, quality: 0, stat_set_index: 0 }] }] },
    { items: [{ slot: 'ring1', text: {} }] },
    { items: [{ slot: 'unknown', text: '' }] },
    { flasks: {} },
    { limbs: {} },
    { limbs: [{ slot: 'Arm 1', text: '' }] },
    { limbs: [{ slot: 'arm3', text: '' }] },
    { jewels: [{ socket_node: '770', text: '' }] },
    { annotations: [] },
    { annotations: { note: {} } },
    { attributeChoices: { '770': 'unknown' } },
    { params: [] },
    { params: { config_inputs: [] } },
    { params: { config_inputs: { conditionFullLife: {} } } },
    { params: { extra_modifiers: [false] } },
    { params: { custom_modifier_blocks: null } },
    { params: { custom_modifier_blocks: {} } },
    { params: { custom_modifier_blocks: [null] } },
    { params: { custom_modifier_blocks: [{ title: 'Active', enabled: true }] } },
    { params: { custom_modifier_blocks: [{ title: 1, enabled: true, text: '+10 to maximum Life' }] } },
    { params: { custom_modifier_blocks: [{ title: 'Active', enabled: 'true', text: '+10 to maximum Life' }] } },
    { params: { custom_modifier_blocks: [{ title: 'Active', enabled: true, text: ['+10 to maximum Life'] }] } },
    { params: { enemy_tier: 'invalid' } },
    { weaponSwap: { active: 3, alternate_items: [], exclusive_nodes: [[], []] } },
  ])('rejects malformed nested state %j', patch => {
    expect(parseState(patch)).toBeNull();
  });

  test('rejects an entire shared workspace when any stage is malformed', () => {
    const workspace = createWorkspace(saved);
    const malformed = structuredClone(workspace);
    (malformed.builds[0].stages[0].saved.state as unknown as Record<string, unknown>).jewels = {};
    expect(parseWorkspace(malformed, parseSaved)).toBeNull();
    expect(workspace.builds[0].stages[0].saved).toEqual(saved);
  });
});


test('restores source groups from unchanged legacy lines without overwriting edited or cleared saves', () => {
  const blocks = [
    { title: 'Active', enabled: true, text: '+100 to maximum Life' },
    { title: 'Later', enabled: false, text: '+200 to maximum Life' },
  ];
  const decoded = { config_inputs: {}, custom_modifier_blocks: blocks, tree: {}, socket_groups: [] } as unknown as BuildJson;
  const state = { ...saved.state, params: { config_inputs: {}, extra_modifiers: ['+100 to maximum Life'] } };
  expect(backfillFromDecoded(state, decoded).params.custom_modifier_blocks).toEqual(blocks);
  const changed = { ...state, params: { ...state.params, extra_modifiers: ['+50 to maximum Life'] } };
  expect(backfillFromDecoded(changed, decoded).params.extra_modifiers).toEqual(['+50 to maximum Life']);
  expect(backfillFromDecoded(changed, decoded).params.custom_modifier_blocks).toBeUndefined();
  const cleared = { ...state, params: { config_inputs: {}, custom_modifier_blocks: [] } };
  expect(backfillFromDecoded(cleared, decoded).params.custom_modifier_blocks).toEqual([]);
  expect(backfillFromDecoded(cleared, decoded).params.extra_modifiers).toBeUndefined();
});

describe('unresolved imported gem persistence', () => {
  const unresolved: SavedSession = {
    ...saved,
    notes: 'Imported Firestorm with an unresolved Unleash slot',
    state: {
      ...saved.state,
      character: { class_name: 'Sorceress', ascendancy_name: 'Stormweaver', level: 98 },
      socketGroups: [{
        enabled: true, slot: null, source: null, main_active_skill: 1,
        gems: [
          { skill_id: 'FirestormPlayer', level: 19, quality: 20 },
          { skill_id: '', level: 0, quality: 0, name_spec: 'Unleash' },
          { skill_id: 'SupportSpellCascadePlayer', level: 1, quality: 0, stat_set_index: 2 },
        ],
      }],
      annotations: { 'skill:0': 'Do not discard the unresolved middle slot' },
      params: {
        main_socket_group: 0, enemy_tier: 'boss',
        config_inputs: { conditionEnemyColdExposure: true, conditionFullLife: false, rawNumber: '23', multiplier: 0 },
        custom_modifier_blocks: [{ title: 'Paused', enabled: false, text: '\n+10 to maximum Life\n' }],
      },
      weaponSwap: { active: 1, alternate_items: [], exclusive_nodes: [[], []] },
    },
  };
  const workspace = {
    version: 1 as const, activeBuild: 'other-build', builds: [
      {
        id: 'imported-build', name: 'Comet', activeStage: 'edited-stage',
        stages: [
          { id: 'imported-stage', name: 'Imported', saved: unresolved, drafts: { ring1: 'Unapplied draft' } },
          { id: 'edited-stage', name: 'Edited', saved: {
            ...unresolved, notes: 'Deliberately edited notes',
            state: { ...unresolved.state, character: { ...unresolved.state.character, level: 99 } },
          }, drafts: {} },
        ],
      },
      {
        id: 'other-build', name: 'Other build', activeStage: 'other-stage',
        stages: [{ id: 'other-stage', name: 'Starter', saved, drafts: {} }],
      },
    ],
  };

  test('preserves an empty-ID level-zero middle slot and the complete saved stage without coercion', () => {
    const raw = JSON.stringify(unresolved);
    const restored = parseSaved(raw);
    expect(restored).toEqual(unresolved);
  });

  test('preserves inactive unresolved stages, deliberate edits and unrelated builds in a workspace', () => {
    const raw = JSON.stringify(workspace);
    expect(parseWorkspace(JSON.parse(raw), parseSaved)).toEqual(workspace);
  });

  test.each([
    { skill_id: 'FirestormPlayer', level: 0 },
    { skill_id: ' ', level: 0 },
    ...[-1, 0.5, '0', 0x100000000].map(level => ({ skill_id: '', level })),
    ...[-1, 0.5, '20', 0x100000000].map(level => ({ skill_id: 'FirestormPlayer', level })),
    ...[-1, 0.5, '0', 0x100000000].map(quality => ({ quality })),
    ...[0, -1, 0.5, '1', 0x100000000].map(stat_set_index => ({ stat_set_index })),
    ...[1, true, [], {}].map(name_spec => ({ name_spec })),
    ...[1, 'true', [], {}].map(corrupted => ({ corrupted })),
    ...[0, -1, 0.5, '2', []].map(minion_skill => ({ minion_skill })),
  ])('rejects malformed gem values in standalone and inactive workspace stages: %j', patch => {
    const invalid = structuredClone(unresolved);
    Object.assign(invalid.state.socketGroups[0].gems[1], patch);
    expect(parseSaved(JSON.stringify(invalid))).toBeNull();
    const invalidWorkspace = structuredClone(workspace);
    invalidWorkspace.builds[0].stages[0].saved = invalid;
    expect(parseWorkspace(invalidWorkspace, parseSaved)).toBeNull();
  });

  test.each([
    { items: [{ slot: 'ring1', text: {} }] },
    { flasks: {} },
    { jewels: [{ socket_node: -1, text: 'Invalid socket' }] },
    { params: { config_inputs: { conditionFullLife: {} } } },
    { annotations: { 'skill:0': [] } },
  ])('does not excuse malformed unrelated state when an unresolved gem is present: %j', patch => {
    const invalid = { ...unresolved, state: { ...unresolved.state, ...patch } };
    expect(parseSaved(JSON.stringify(invalid))).toBeNull();
    const invalidWorkspace = structuredClone(workspace);
    Object.assign(invalidWorkspace.builds[0].stages[0].saved.state, patch);
    expect(parseWorkspace(invalidWorkspace, parseSaved)).toBeNull();
  });

  test('keeps positive unresolved levels and uint32 gem metadata without adding game-level caps', () => {
    const session = structuredClone(unresolved);
    session.state.socketGroups[0].gems[1] = {
      skill_id: '', level: 0xffffffff, quality: 0xffffffff, stat_set_index: 0xffffffff,
    };
    expect(parseSaved(JSON.stringify(session))).toEqual(session);
    const restoredWorkspace = structuredClone(workspace);
    restoredWorkspace.builds[0].stages[0].saved = session;
    expect(parseWorkspace(restoredWorkspace, parseSaved)).toEqual(restoredWorkspace);
  });

  test.each([undefined, null, '', 'Unleash'])('preserves optional source name metadata %j', name_spec => {
    const session = structuredClone(unresolved);
    session.state.socketGroups[0].gems[1].name_spec = name_spec;
    expect(parseSaved(JSON.stringify(session))).toEqual(session);
  });

  test('keeps Full DPS inclusion and instance counts through a save', () => {
    const session = structuredClone(unresolved);
    Object.assign(session.state.socketGroups[0], { include_in_full_dps: true, group_count: 4.5 });
    Object.assign(session.state.socketGroups[0].gems[0], { enabled: false, count: 2.5, stat_set_index: 2, enable_global_1: false, enable_global_2: true });
    expect(parseSaved(JSON.stringify(session))).toEqual(session);
  });

  test('keeps the minion and the minion skill a gem chose through a save', () => {
    const session = structuredClone(unresolved);
    Object.assign(session.state.socketGroups[0].gems[0], {
      minion: 'Metadata/Monsters/Quadrilla/Quadrilla', minion_skill: 3,
    });
    expect(parseSaved(JSON.stringify(session))).toEqual(session);
  });

  test('keeps whether a gem is corrupted through a save and an import', () => {
    const session = structuredClone(unresolved);
    session.state.socketGroups[0].gems[0].corrupted = true;
    expect(parseSaved(JSON.stringify(session))).toEqual(session);
    // A gem saved as not corrupted is the same as one that does not say.
    const clean = structuredClone(unresolved);
    (clean.state.socketGroups[0].gems[0] as { corrupted?: boolean }).corrupted = false;
    expect(parseSaved(JSON.stringify(clean))).toEqual(unresolved);

    const decoded = {
      config_inputs: {}, tree: {}, items: { equipped: [], socket_jewels: [], flasks: [] },
      socket_groups: session.state.socketGroups,
    } as unknown as BuildJson;
    const gems = materialize(decoded).socketGroups[0].gems;
    expect(gems.map(gem => gem.corrupted)).toEqual([true, undefined, undefined]);
  });

  test('backfills an old unnamed slot only for intact gem order and keeps deliberate name edits', () => {
    const decoded = {
      config_inputs: {}, tree: {}, socket_groups: unresolved.state.socketGroups,
    } as unknown as BuildJson;
    const legacy = structuredClone(unresolved.state);
    delete legacy.socketGroups[0].gems[1].name_spec;
    expect(backfillFromDecoded(legacy, decoded).socketGroups[0].gems[1])
      .toEqual(unresolved.state.socketGroups[0].gems[1]);
    const edited = structuredClone(unresolved.state);
    edited.socketGroups[0].gems[1].name_spec = 'Deliberately edited source name';
    expect(backfillFromDecoded(edited, decoded).socketGroups[0].gems[1].name_spec)
      .toBe('Deliberately edited source name');
  });

  test.each(['shifted', 'removed', 'cleared'] as const)('does not resurrect source names or removed gems after %s gem edits', edit => {
    const decoded = {
      config_inputs: {}, tree: {}, socket_groups: unresolved.state.socketGroups,
    } as unknown as BuildJson;
    const state = structuredClone(unresolved.state);
    const gems = state.socketGroups[0].gems;
    delete gems[1].name_spec;
    state.socketGroups[0].gems = edit === 'shifted' ? [gems[1], gems[0], gems[2]]
      : edit === 'removed' ? [gems[0], gems[2]] : [];
    const expected = structuredClone(state.socketGroups[0].gems);
    expect(backfillFromDecoded(state, decoded).socketGroups[0].gems).toEqual(expected);
  });
  test('does not infer names for multiple legacy empty-ID slots after an ambiguous reorder', () => {
    const state = structuredClone(unresolved.state);
    state.socketGroups[0].gems.push({ skill_id: '', level: 2, quality: 7 });
    delete state.socketGroups[0].gems[1].name_spec;
    const original = structuredClone(state.socketGroups);
    original[0].gems[1].name_spec = 'Unleash';
    original[0].gems[3].name_spec = 'Other source';
    [state.socketGroups[0].gems[1], state.socketGroups[0].gems[3]] =
      [state.socketGroups[0].gems[3], state.socketGroups[0].gems[1]];
    const decoded = { config_inputs: {}, tree: {}, socket_groups: original } as unknown as BuildJson;
    expect(backfillFromDecoded(state, decoded).socketGroups[0].gems)
      .toEqual(state.socketGroups[0].gems);
  });
  test.each(['reordered', 'removed'] as const)('does not infer names across indistinguishable source groups after %s groups', edit => {
    const original = structuredClone(unresolved.state.socketGroups);
    const other = structuredClone(original[0]);
    other.gems[1] = { skill_id: '', level: 2, quality: 7, name_spec: 'Other source' };
    original.push(other);
    const state = structuredClone(unresolved.state);
    state.socketGroups = structuredClone(original).reverse();
    for (const group of state.socketGroups) delete group.gems[1].name_spec;
    if (edit === 'removed') state.socketGroups.pop();
    const expected = structuredClone(state.socketGroups);
    const decoded = { config_inputs: {}, tree: {}, socket_groups: original } as unknown as BuildJson;
    expect(backfillFromDecoded(state, decoded).socketGroups).toEqual(expected);
  });
});


test('Full DPS is requested only for included groups, including a computed empty total', () => {
  const state = structuredClone(saved.state);
  expect(toRequest(state).full_dps).toBeUndefined();
  Object.assign(state.socketGroups[0], { enabled: false, include_in_full_dps: true });
  expect(toRequest(state).full_dps).toBe(true);
});


test('complete comparison edits survive saved-session parsing without source config resurrection', () => {
  const edited = { ...saved, state: { ...saved.state, sourceBackfillComplete: true, params: { config_inputs: { zero: 0 } } } };
  const restored = parseSaved(JSON.stringify(edited))!;
  expect(restored.state.sourceBackfillComplete).toBe(true);
  expect(parseSaved(JSON.stringify({ ...edited, state: { ...edited.state, sourceBackfillComplete: 'invalid' } }))).toBeNull();
  expect(backfillFromDecoded(restored.state, { config_inputs: { conditionLowLife: true } } as unknown as BuildJson)).toBe(restored.state);
  expect(restored.state.params.config_inputs).toEqual({ zero: 0 });
});

test('planner source survives saves without entering calculation requests', () => {
  const plannerSource = JSON.stringify({ name: 'Guide', inventory_slots: [{ inventory_id: 'Weapon1', additional_text: 'Choose a bow' }] });
  const restored = parseState({ plannerSource })!;
  expect(restored.state.plannerSource).toBe(plannerSource);
  expect(toRequest(restored.state)).not.toHaveProperty('plannerSource');
  const workspace = parseWorkspace(createWorkspace(restored), parseSaved)!;
  expect(workspace.builds[0].stages[0].saved.state.plannerSource).toBe(plannerSource);
});

test('selecting a named config changes only configuration and its persisted XML base', () => {
  const current = { ...saved.state, params: { ...saved.state.params, main_socket_group: 4, extra_modifiers: ['old'] } };
  const decoded = { code: 'selected-code', config_inputs: { enemyIsBoss: 'Pinnacle', future: false },
    custom_modifier_blocks: [{ title: 'New', enabled: false, text: 'keep' }] } as unknown as BuildJson & { code: string };
  const next = applyNamedConfig(current, decoded);
  expect(next.params).toEqual({ config_inputs: { future: false }, enemy_tier: 'pinnacle',
    custom_modifier_blocks: decoded.custom_modifier_blocks, main_socket_group: 4 });
  expect(next.pobCode).toBe('selected-code');
  for (const key of ['character', 'socketGroups', 'items', 'flasks', 'limbs', 'jewels', 'allocatedNodes', 'attributeChoices', 'weaponSwap', 'annotations'] as const) {
    expect(next[key]).toBe(current[key]);
  }
  expect(parseSaved(JSON.stringify({ ...saved, state: next }))?.state.params).toEqual(next.params);
  expect(toRequest(next).config_inputs).toEqual({ future: false });
  expect(toRequest(next).enemy_tier).toBe('pinnacle');
});

test('selecting a named skill set replaces its main group and bindings while keeping other edits', () => {
  const current = { ...saved.state, params: { ...saved.state.params, main_socket_group: 4, extra_modifiers: ['keep'] } };
  const decoded = { code: 'skill-code', main_socket_group: 1, tree: {}, items: { equipped: [] },
    socket_groups: [
      { enabled: true, weapon_set: 2, gems: [{ skill_id: 'SparkPlayer', level: 10, quality: 5, enable_global_1: false, enable_global_2: true }] },
      { enabled: true, weapon_set: 1, main_active_skill: 2, gems: [{ skill_id: 'FireballPlayer', level: 9, quality: 0 }] },
    ] } as unknown as BuildJson & { code: string };
  const next = applyNamedSkills(current, decoded);
  expect(next.params).toEqual({ ...current.params, main_socket_group: 1 });
  expect(next.socketGroups[0]).toMatchObject({ weapon_set: 2, gems: [{ skill_id: 'SparkPlayer', level: 10, enable_global_2: true }] });
  expect(next.socketGroups[1]).toMatchObject({ weapon_set: 1, main_active_skill: 2 });
  expect(next.pobCode).toBe('skill-code');
  for (const key of ['character', 'items', 'flasks', 'limbs', 'jewels', 'allocatedNodes', 'attributeChoices', 'weaponSwap', 'annotations'] as const) {
    expect(next[key]).toBe(current[key]);
  }
  const restored = parseSaved(JSON.stringify({ ...saved, state: next }));
  expect(restored?.state.socketGroups).toEqual(next.socketGroups);
  expect(toRequest(next).main_socket_group).toBe(1);
  expect(toRequest(next).config_inputs).toEqual(current.params.config_inputs);
});
