import { expect, test } from 'vitest';
import { applyTreeAllocation, treeAllocationRequest, groupsForWeaponSet, skillWeaponSet, switchWeapons, validWeaponSwap, weaponSetRequest, type WeaponSetState } from './weaponSets';
const state: WeaponSetState = {
  items: [{ slot: 'weapon1', text: 'Bow' }, { slot: 'weapon2', text: 'Quiver' }, { slot: 'ring1', text: 'Ring' }],
  allocatedNodes: [10, 11], socketGroups: [{ enabled: true, weapon_set: 2, gems: [] }],
  weaponSwap: { active: 1, alternate_items: [{ slot: 'weapon1', text: 'Staff' }], exclusive_nodes: [[11], [22]] },
};
test('swapping preserves shared gear/passives but removes the other pair and exclusive nodes', () => {
  const second = switchWeapons(state, 2);
  expect(second.items).toEqual([{ slot: 'ring1', text: 'Ring' }, { slot: 'weapon1', text: 'Staff' }]);
  expect(second.allocatedNodes).toEqual([10, 22]);
  expect(state.items).toHaveLength(3);
  expect(switchWeapons(second, 1).items).toEqual([state.items[2], state.items[0], state.items[1]]);
  expect(switchWeapons(second, 1).allocatedNodes).toEqual(state.allocatedNodes);
});
test('skill bindings choose their set, while shared skills follow the active set', () => {
  expect(skillWeaponSet(state, 0)).toBe(2);
  expect(skillWeaponSet(state, 1)).toBe(1);
  expect(skillWeaponSet(state)).toBe(1);
  expect(skillWeaponSet(switchWeapons(state, 2))).toBe(2);
  const groups = [...state.socketGroups, { enabled: true, gems: [] }, { enabled: false, gems: [] }];
  expect(groupsForWeaponSet(groups, 1).map(g => g.enabled)).toEqual([false, true, false]);
  expect(groupsForWeaponSet(groups, 2).map(g => g.enabled)).toEqual([true, true, false]);
});
test('a request names the set its groups are enabled for, the first when weapons never swapped', () => {
  const groups = [...state.socketGroups, { enabled: true, gems: [] }];
  const first = weaponSetRequest({ socketGroups: groups, weaponSwap: state.weaponSwap });
  expect(first.weapon_swap).toEqual(state.weaponSwap);
  expect(first.socket_groups.map(g => g.enabled)).toEqual([false, true]);
  const second = weaponSetRequest(switchWeapons({ ...state, socketGroups: groups }, 2));
  expect(second.weapon_swap).toEqual({ active: 2, alternate_items: [state.items[0], state.items[1]], exclusive_nodes: [[11], [22]] });
  expect(second.socket_groups.map(g => g.enabled)).toEqual([true, true]);
  expect(weaponSetRequest({ socketGroups: groups, weaponSwap: null }).weapon_swap)
    .toEqual({ active: 1, alternate_items: [], exclusive_nodes: [[], []] });
  // A restored save may carry other keys; the request schema rejects them.
  const saved = { ...state.weaponSwap!, note: 'local' };
  expect(Object.keys(weaponSetRequest({ socketGroups: groups, weaponSwap: saved }).weapon_swap))
    .toEqual(['active', 'alternate_items', 'exclusive_nodes']);
});
test('older saves start with an empty second set and reject malformed swap data', () => {
  expect(switchWeapons({ ...state, weaponSwap: undefined }, 2).items).toEqual([state.items[2]]);
  expect(validWeaponSwap({ active: 3 })).toBeUndefined();
  expect(validWeaponSwap(state.weaponSwap)).toBe(state.weaponSwap);
});
test('Full DPS keeps the original enabled state of an included alternate-set group', () => {
  const groups = groupsForWeaponSet([
    { enabled: true, weapon_set: 2, include_in_full_dps: true, gems: [] },
    { enabled: false, weapon_set: 2, include_in_full_dps: true, gems: [] },
  ], 1);
  expect(groups.map(group => group.enabled)).toEqual([false, false]);
  expect(groups.map(group => group.full_dps_enabled)).toEqual([true, false]);
});

test('validated tree edits keep inactive allocations and alternate equipment through swaps', () => {
  const edited = applyTreeAllocation(state, { shared: [10, 11], exclusive: [[12], [22, 23]] });
  expect(edited.allocatedNodes).toEqual([10, 11, 12]);
  expect(edited.weaponSwap?.alternate_items).toEqual(state.weaponSwap?.alternate_items);
  const second = switchWeapons(edited, 2);
  expect(second.allocatedNodes).toEqual([10, 11, 22, 23]);
  expect(switchWeapons(second, 1).allocatedNodes).toEqual(edited.allocatedNodes);
  expect(switchWeapons(second, 1).weaponSwap).toEqual(edited.weaponSwap);
  const preview = treeAllocationRequest({ allocated_nodes: [10, 11], weapon_swap: state.weaponSwap!,
    attribute_choices: { '11': 'str', '22': 'dex' }, jewels: [{ socket_node: 11, text: 'Jewel' }] },
  { shared: [10], exclusive: [[], [22]] });
  expect(preview.allocated_nodes).toEqual([10]);
  expect(preview.attribute_choices).toEqual({ '22': 'dex' });
  expect(preview.jewels).toEqual([]);
  expect(preview.weapon_swap?.exclusive_nodes).toEqual([[], [22]]);
});
