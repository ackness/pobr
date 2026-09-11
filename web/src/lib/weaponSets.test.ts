import { expect, test } from 'vitest';
import { groupsForWeaponSet, skillWeaponSet, switchWeapons, validWeaponSwap, type WeaponSetState } from './weaponSets';
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
  const groups = [...state.socketGroups, { enabled: true, gems: [] }, { enabled: false, gems: [] }];
  expect(groupsForWeaponSet(groups, 1).map(g => g.enabled)).toEqual([false, true, false]);
  expect(groupsForWeaponSet(groups, 2).map(g => g.enabled)).toEqual([true, true, false]);
});
test('older saves start with an empty second set and reject malformed swap data', () => {
  expect(switchWeapons({ ...state, weaponSwap: undefined }, 2).items).toEqual([state.items[2]]);
  expect(validWeaponSwap({ active: 3 })).toBeUndefined();
  expect(validWeaponSwap(state.weaponSwap)).toBe(state.weaponSwap);
});
