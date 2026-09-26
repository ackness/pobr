import { describe, expect, test } from 'vitest';
import { backfillFromDecoded, paramsFromDecoded, parseSaved, type SavedSession } from './useBuildSession';
import type { BuildJson } from '../api/types';
import { parseWorkspace, createWorkspace } from '../lib/buildWorkspace';

const saved: SavedSession = {
  version: 1, notes: 'Keep this build', state: {
    character: { class_name: 'Witch', ascendancy_name: '', level: 1 }, pobCode: null,
    treeVersion: '0_1', allocatedNodes: [770], attributeChoices: { '770': 'int' },
    socketGroups: [{ enabled: true, gems: [{ skill_id: 'IceNovaPlayer', level: 20, quality: 0, stat_set_index: 2 }] }],
    items: [{ slot: 'ring1', text: 'Rarity: NORMAL\nSapphire Ring' }], flasks: [], jewels: [],
    annotations: {}, params: { config_inputs: { conditionFullLife: false, multiplier: 0 } },
  },
};

const parseState = (patch: Record<string, unknown>) => parseSaved(JSON.stringify({ ...saved, state: { ...saved.state, ...patch } }));

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
      flasks: undefined, jewels: undefined, annotations: undefined });
    expect(restored?.state.params.config_inputs).toEqual({});
    expect(Object.keys(restored!.state.params.config_inputs)).toEqual([]);
    expect(restored?.state.attributeChoices).toEqual({});
    expect(restored?.state.flasks).toEqual([]);
    expect(restored?.state.jewels).toEqual([]);
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
