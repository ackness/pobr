import type { PassiveAllocationGrant, PassiveNode, WeaponSwap } from '../../api/types';
import { connectedAllocation, type PassiveGraph } from '../../lib/passiveGraph';

export function isDisconnectedTree(reason: string | null | undefined): boolean {
  return reason === 'disconnected_shared_path' || reason === 'disconnected_weapon_path';
}

export function needsTreeConversion(reason: string | null | undefined): boolean {
  return ['overlapping_allocations', 'shared_only_node', 'unknown_node'].includes(reason ?? '');
}

export function isImportProblem(reason: string | null | undefined): boolean {
  return isDisconnectedTree(reason) || needsTreeConversion(reason);
}

/** Check each complete weapon set independently: the other set cannot supply a path. */
export function hasDisconnectedTreeNodes(nodes: readonly PassiveNode[], graph: PassiveGraph, root: number | null,
  grants: readonly PassiveAllocationGrant[], allocated: readonly number[], swap?: WeaponSwap | null): boolean {
  if (!nodes.length || root === null) return false;
  const byId = new Map(nodes.map(node => [node.skill, node]));
  const exclusive = new Set(swap?.exclusive_nodes.flat() ?? []);
  const shared = allocated.filter(id => !exclusive.has(id));
  const sets = swap ? swap.exclusive_nodes.map(ids => [...shared, ...ids]) : [allocated];
  return sets.some(ids => {
    const main = new Set(ids.filter(id => !byId.get(id)?.ascendancy_id));
    const connected = connectedAllocation(graph, main, root, grants);
    return [...main].some(id => !connected.has(id));
  });
}
