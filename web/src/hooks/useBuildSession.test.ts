import { describe, expect, test } from 'vitest';
import { parseSaved, type SavedSession } from './useBuildSession';
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
