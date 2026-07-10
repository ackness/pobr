import type { PassiveNode } from '../api/types';

export type PassiveGraph = Map<number, number[]>;

const CLASS_START: Record<string, string> = {
  Druid: 'TEMPLAR',
  Templar: 'TEMPLAR',
  Duelist: 'DUELIST',
  Mercenary: 'DUELIST',
  Huntress: 'RANGER',
  Ranger: 'RANGER',
  Marauder: 'MARAUDER',
  Warrior: 'MARAUDER',
  Monk: 'SIX',
  Shadow: 'SIX',
  Sorceress: 'WITCH',
  Witch: 'WITCH',
};

/** GGG connections are one-way records; the passive tree is traversed as an undirected graph. */
export function buildPassiveGraph(nodes: PassiveNode[]): PassiveGraph {
  const byId = new Map(nodes.map((node) => [node.skill, node]));
  const graph: PassiveGraph = new Map(nodes.map((node) => [node.skill, []]));
  for (const node of nodes) {
    for (const other of node.connections ?? []) {
      const target = byId.get(other);
      if (!target || (node.ascendancy_id ?? null) !== (target.ascendancy_id ?? null)) continue;
      graph.get(node.skill)!.push(other);
      graph.get(other)!.push(node.skill);
    }
  }
  for (const neighbours of graph.values()) {
    neighbours.sort((a, b) => a - b);
    for (let i = neighbours.length - 1; i > 0; i -= 1) {
      if (neighbours[i] === neighbours[i - 1]) neighbours.splice(i, 1);
    }
  }
  return graph;
}

export function classStartSkill(nodes: PassiveNode[], className?: string): number | null {
  const startName = className ? CLASS_START[className] : undefined;
  if (!startName) return null;
  return nodes.find((node) => node.name === startName)?.skill ?? null;
}

/** Deterministic multi-source BFS. The result contains only nodes that must be newly allocated. */
export function shortestAllocationPath(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  target: number,
): number[] {
  if (allocated.has(target)) return [];
  const sources = new Set(allocated);
  if (root !== null) sources.add(root);
  if (sources.size === 0) return [target];

  const queue = [...sources].filter((id) => graph.has(id)).sort((a, b) => a - b);
  const previous = new Map<number, number | null>(queue.map((id) => [id, null]));
  for (let index = 0; index < queue.length; index += 1) {
    const current = queue[index];
    if (current === target) break;
    for (const next of graph.get(current) ?? []) {
      if (previous.has(next)) continue;
      previous.set(next, current);
      queue.push(next);
    }
  }
  if (!previous.has(target)) return [target];

  const path: number[] = [];
  let current: number | null = target;
  while (current !== null && !sources.has(current)) {
    path.push(current);
    current = previous.get(current) ?? null;
  }
  return path.reverse();
}

/** DFS from the implicit root after a removal; disconnected allocated branches are removed together. */
export function connectedAllocation(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
): Set<number> {
  if (root === null || !graph.has(root)) return new Set(allocated);
  const reachable = new Set<number>();
  if (allocated.has(root)) reachable.add(root);
  const stack = [root];
  const visited = new Set(stack);
  while (stack.length > 0) {
    const current = stack.pop()!;
    for (const next of graph.get(current) ?? []) {
      if (visited.has(next) || !allocated.has(next)) continue;
      visited.add(next);
      reachable.add(next);
      stack.push(next);
    }
  }
  return reachable;
}

/**
 * 取消一个已加点节点。仅当图模型能完整解释当前加点全集（全部可达根）时才做
 * 级联清理；导入的真实 build 常含模型外连接（class 起点挂接边缺失、武器组加点），
 * 此时退化为单点取消，绝不误删整棵树。
 */
export function deallocateNode(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  removed: number,
): Set<number> {
  const remaining = new Set(allocated);
  remaining.delete(removed);
  if (connectedAllocation(graph, allocated, root).size !== allocated.size) return remaining;
  return connectedAllocation(graph, remaining, root);
}
