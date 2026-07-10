import { describe, expect, it } from 'vitest';
import type { PassiveNode } from '../api/types';
import {
  buildPassiveGraph,
  classStartSkill,
  connectedAllocation,
  deallocateNode,
  shortestAllocationPath,
} from './passiveGraph';

const node = (skill: number, connections: number[] = [], name?: string): PassiveNode => ({
  skill,
  id: String(skill),
  name,
  kind: 'normal',
  connections,
});

describe('passive graph', () => {
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

  it('cascades deallocation when the whole allocation is root-connected', () => {
    const graph = buildPassiveGraph([node(1, [2]), node(2, [3]), node(3, [4]), node(4)]);
    expect([...deallocateNode(graph, new Set([2, 3, 4]), 1, 2)]).toEqual([]);
  });

  it('falls back to single-node removal when the model cannot explain the allocation', () => {
    // 导入的真实 build：加点集合与 class 起点在模型里不连通（挂接边缺失/武器组加点）。
    const graph = buildPassiveGraph([node(1), node(5, [6]), node(6, [7]), node(7)]);
    const result = deallocateNode(graph, new Set([5, 6, 7]), 1, 7);
    expect([...result].sort()).toEqual([5, 6]);
  });

  it('maps current PoE2 classes to their shared start nodes', () => {
    const nodes = [node(10, [], 'MARAUDER'), node(20, [], 'WITCH')];
    expect(classStartSkill(nodes, 'Warrior')).toBe(10);
    expect(classStartSkill(nodes, 'Sorceress')).toBe(20);
  });
});
