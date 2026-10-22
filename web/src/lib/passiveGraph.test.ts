import { describe, expect, it } from 'vitest';
import type { PassiveJewelState, PassiveNode } from '../api/types';
import {
  buildPassiveGraph,
  classStartSkill,
  connectedAllocation,
  shortestAllocationPath,
  reconcileJewelAllocation,
} from './passiveGraph';

const node = (skill: number, connections: number[] = [], name?: string): PassiveNode => ({
  skill,
  id: String(skill),
  name,
  kind: 'normal',
  connections,
});

describe('passive graph', () => {
  it('reconciles changed jewels while preserving overlapping grants and unexplained imports', () => {
    const nodes = [node(1, [2, 3]), node(2), node(3), node(10, [11]), node(11), node(12), node(99)];
    const state = (allocation_grants: PassiveJewelState['allocation_grants']): PassiveJewelState => ({
      class_starts: { witch: 1 }, allocation_grants, nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [],
    });
    const before = state([{ source: 2, nodes: [12], roots: [10] }, { source: 3, nodes: [12], roots: [] }]);
    const after = state([{ source: 3, nodes: [12], roots: [] }]);
    expect(reconcileJewelAllocation(nodes, [2, 3, 11, 12, 99], 'Witch', before, after)).toEqual([2, 3, 12, 99]);
    expect(reconcileJewelAllocation(nodes, [2, 3, 11, 12, 99], 'Witch', before, state([]))).toEqual([2, 3, 99]);
    expect(reconcileJewelAllocation(nodes, [2, 3, 11, 12, 99], 'Witch', before)).toEqual([2, 3, 11, 12, 99]);
  });
  it('allocates radius nodes independently and does not extend paths from a disconnected radius node', () => {
    const graph = buildPassiveGraph([node(1, [2, 3]), node(2), node(3, [4]), node(4, [5]), node(5, [6]), node(6)]);
    const grants = [{ source: 2, nodes: [5], roots: [] }];
    expect(shortestAllocationPath(graph, new Set([2]), 1, 5, grants)).toEqual([5]);
    expect(shortestAllocationPath(graph, new Set([2, 5]), 1, 6, grants)).toEqual([3, 4, 6]);
    expect([...connectedAllocation(graph, new Set([2, 5, 6]), 1, grants)].sort()).toEqual([2, 5]);
    expect([...connectedAllocation(graph, new Set([5]), 1, grants)]).toEqual([]);
    expect([...connectedAllocation(graph, new Set([3, 4, 5]), 1, grants)].sort()).toEqual([3, 4, 5]);
  });

  it('uses an extra implicit class start only while its provider is connected', () => {
    const graph = buildPassiveGraph([node(1, [2]), node(2), node(10, [11]), node(11, [12]), node(12)]);
    const grants = [{ source: 2, nodes: [], roots: [10] }];
    expect(shortestAllocationPath(graph, new Set([2]), 1, 12, grants)).toEqual([11, 12]);
    expect([...connectedAllocation(graph, new Set([2, 11, 12]), 1, grants)].sort((a,b)=>a-b)).toEqual([2, 11, 12]);
    expect([...connectedAllocation(graph, new Set([11, 12]), 1, grants)]).toEqual([]);
    expect(shortestAllocationPath(graph, new Set(), 1, 12, grants)).toEqual([]);
  });

  it('rejects circular jewel dependencies and retains radius nodes with a second connected provider', () => {
    const graph = buildPassiveGraph([node(1, [2, 3]), node(2), node(3), node(10, [11]), node(11)]);
    const circular = [{ source: 11, nodes: [], roots: [10] }];
    expect(connectedAllocation(graph, new Set([11]), 1, circular).size).toBe(0);
    const grants = [{ source: 2, nodes: [11], roots: [] }, { source: 3, nodes: [11], roots: [] }];
    expect([...connectedAllocation(graph, new Set([3, 11]), 1, grants)].sort((a,b)=>a-b)).toEqual([3, 11]);
  });
  it('fills reverse edges and finds a deterministic shortest path', () => {
    const graph = buildPassiveGraph([
      node(1, [3, 2]),
      node(2, [4]),
      node(3, [4]),
      node(4),
    ]);
    expect(graph.get(2)).toContain(1);
    expect(shortestAllocationPath(graph, new Set([1]), 1, 4)).toEqual([2, 4]);
  });

  it('uses all allocated nodes as BFS sources', () => {
    const graph = buildPassiveGraph([node(1, [2]), node(2, [3]), node(3, [4]), node(4)]);
    expect(shortestAllocationPath(graph, new Set([1, 3]), 1, 4)).toEqual([4]);
  });

  it('drops branches disconnected by a removed middle node', () => {
    const graph = buildPassiveGraph([node(1, [2]), node(2, [3, 4]), node(3), node(4)]);
    expect([...connectedAllocation(graph, new Set([3, 4]), 1)]).toEqual([]);
    expect([...connectedAllocation(graph, new Set([2, 4]), 1)].sort()).toEqual([2, 4]);
  });

  it('keeps an allocated root in the connected set', () => {
    const graph = buildPassiveGraph([node(1, [2]), node(2, [3]), node(3)]);
    expect([...connectedAllocation(graph, new Set([1, 2, 3]), 1)].sort()).toEqual([1, 2, 3]);
  });

  it('maps current PoE2 classes to their shared start nodes', () => {
    const nodes = [node(10, [], 'MARAUDER'), node(20, [], 'WITCH')];
    expect(classStartSkill(nodes, 'Warrior')).toBe(10);
    expect(classStartSkill(nodes, 'Sorceress')).toBe(20);
  });
});
