import { expect, test } from 'vitest';
import type { PassiveNode, WeaponSwap } from '../../api/types';
import { buildPassiveGraph } from '../../lib/passiveGraph';
import { hasDisconnectedTreeNodes, isDisconnectedTree, needsTreeConversion } from './treeImportStatus';

const node = (skill: number, connections: number[] = [], extra: Partial<PassiveNode> = {}): PassiveNode =>
  ({ id: String(skill), skill, kind: 'normal', connections, ...extra });
const nodes = [node(1, [2]), node(2, [3]), node(3, [4]), node(4)];
const graph = buildPassiveGraph(nodes);
const swap: WeaponSwap = { active: 1, exclusive_nodes: [[2, 3], [4]], alternate_items: [] };

test('warns on a disconnected branch in the inactive set before the player hovers or edits', () => {
  // The active set reaches 3, but its path cannot connect node 4 in the other set.
  expect(hasDisconnectedTreeNodes(nodes, graph, 1, [], [1, 2, 3], swap)).toBe(true);
  expect(hasDisconnectedTreeNodes(nodes, graph, 1, [], [1, 4], { ...swap, active: 2 })).toBe(true);
  expect(hasDisconnectedTreeNodes(nodes, graph, 1, [], [1, 2, 3], { ...swap, exclusive_nodes: [[2, 3], []] })).toBe(false);
});

test('reconnecting or refunding retained disconnected nodes clears the warning without conversion', () => {
  expect(needsTreeConversion('protected_root')).toBe(false);
  expect(needsTreeConversion('disconnected_weapon_path')).toBe(false);
  expect(isDisconnectedTree('disconnected_weapon_path')).toBe(true);
  expect(isDisconnectedTree('disconnected_shared_path')).toBe(true);
  expect(needsTreeConversion('unknown_node')).toBe(true);
  // Move the connecting path into shared nodes so both allocations are also legal.
  const reconnected = { ...swap, exclusive_nodes: [[], [4]] as [number[], number[]] };
  expect(hasDisconnectedTreeNodes(nodes, graph, 1, [], [1, 2, 3], reconnected)).toBe(false);
  expect(hasDisconnectedTreeNodes(nodes, graph, 1, [], [1, 2, 3], { ...swap, exclusive_nodes: [[2, 3], []] })).toBe(false);
});

test('checks shared paths, skips ascendancy nodes, and applies radius grants only in the set with their provider', () => {
  const tree = [...nodes, node(8, [], { ascendancy_id: 'A' }), node(9)];
  const treeGraph = buildPassiveGraph(tree);
  expect(hasDisconnectedTreeNodes(tree, treeGraph, 1, [], [1, 2, 8])).toBe(false);
  expect(hasDisconnectedTreeNodes(tree, treeGraph, 1, [], [1, 4])).toBe(true);
  const grants = [{ source: 2, nodes: [9], roots: [] }];
  expect(hasDisconnectedTreeNodes(tree, treeGraph, 1, grants, [1, 2, 9])).toBe(false);
  expect(hasDisconnectedTreeNodes(tree, treeGraph, 1, grants, [1, 2], { ...swap, exclusive_nodes: [[2], [9]] })).toBe(true);
});
