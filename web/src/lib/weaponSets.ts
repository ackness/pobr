import type { SlotItemInput, SocketGroupInput, WeaponSwap } from '../api/types';

export interface WeaponSetState {
  items: SlotItemInput[];
  allocatedNodes: number[];
  socketGroups: SocketGroupInput[];
  weaponSwap?: WeaponSwap | null;
}

const isWeapon = (item: SlotItemInput) => item.slot === 'weapon1' || item.slot === 'weapon2';

/** Shared equipment and passives remain active in either set. Empty alternate
 * slots stay empty; a shield/quiver must never leak across a weapon swap.
 */
export function switchWeapons<T extends WeaponSetState>(state: T, target: 1 | 2): T {
  const swap = state.weaponSwap ?? { active: 1, alternate_items: [], exclusive_nodes: [[], []] };
  if (swap.active === target) return state;
  return {
    ...state,
    items: [...state.items.filter(item => !isWeapon(item)), ...swap.alternate_items],
    allocatedNodes: [...new Set([
      ...state.allocatedNodes.filter(node => !swap.exclusive_nodes[swap.active - 1].includes(node)),
      ...swap.exclusive_nodes[target - 1],
    ])],
    weaponSwap: { ...swap, active: target, alternate_items: state.items.filter(isWeapon) },
  };
}

export function skillWeaponSet(state: WeaponSetState, index?: number): 1 | 2 {
  return (index === undefined ? undefined : state.socketGroups[index]?.weapon_set) ?? state.weaponSwap?.active ?? 1;
}

/** A calculation contains only the groups enabled for that weapon set. Keep
 * their indices stable for the sidebar, skill metrics, and optimizer variants.
 */
export function groupsForWeaponSet(groups: SocketGroupInput[], active: 1 | 2): SocketGroupInput[] {
  return groups.map(group => ({ ...group,
    ...(group.include_in_full_dps ? { full_dps_enabled: group.enabled } : {}),
    enabled: group.enabled && (!group.weapon_set || group.weapon_set === active) }));
}

/** The request fields for the weapon set in use: its enabled groups, and
 * `weapon_swap`, whose `active` set the engine calculates, applying "while
 * using Weapon Set 1/2" modifiers. A build that never swapped uses set 1.
 */
export function weaponSetRequest(state: Pick<WeaponSetState, 'socketGroups' | 'weaponSwap'>):
  { socket_groups: SocketGroupInput[]; weapon_swap: WeaponSwap } {
  const swap = state.weaponSwap;
  const active = swap?.active ?? 1;
  return {
    socket_groups: groupsForWeaponSet(state.socketGroups, active),
    // Only the fields the request schema knows; it rejects any other key.
    weapon_swap: { active, alternate_items: swap?.alternate_items ?? [], exclusive_nodes: swap?.exclusive_nodes ?? [[], []] },
  };
}

export function validWeaponSwap(value: unknown): WeaponSwap | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const swap = value as WeaponSwap;
  if (![1, 2].includes(swap.active) || !Array.isArray(swap.alternate_items) ||
      swap.alternate_items.some(item => !item || !isWeapon(item) || typeof item.text !== 'string') ||
      !Array.isArray(swap.exclusive_nodes) || swap.exclusive_nodes.length !== 2 ||
      swap.exclusive_nodes.some(nodes => !Array.isArray(nodes) || nodes.some(node => !Number.isInteger(node) || node < 0 || node > 0xffffffff))) return undefined;
  return swap;
}
