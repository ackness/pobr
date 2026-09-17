import type { PassiveAllocationGrant, PassiveJewelState, PassiveNode } from '../api/types';

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

/** Reach class starts first. Radius-only allocations never provide onward paths. */
export function allocationAccess(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  grants: readonly PassiveAllocationGrant[] = [],
): { connected: Set<number>; free: Set<number>; roots: Set<number> } {
  const roots = new Set(root === null ? [] : [root]);
  const connected = new Set<number>();
  const visit = () => {
    const queue = [...roots];
    const seen = new Set(queue);
    for (let i = 0; i < queue.length; i += 1) {
      const id = queue[i];
      if (allocated.has(id)) connected.add(id);
      for (const next of graph.get(id) ?? []) {
        if (!allocated.has(next) || seen.has(next)) continue;
        seen.add(next); queue.push(next);
      }
    }
  };
  visit();
  let changed = true;
  while (changed) {
    changed = false;
    for (const grant of grants) {
      if (!connected.has(grant.source)) continue;
      for (const start of grant.roots) {
        if (!roots.has(start) && graph.has(start)) { roots.add(start); changed = true; }
      }
    }
    if (changed) visit();
  }
  const free = new Set<number>();
  for (const grant of grants) {
    if (connected.has(grant.source)) grant.nodes.forEach(id => { if (graph.has(id)) free.add(id); });
  }
  return { connected, free, roots };
}

/** Shortest physical routes plus independent one-point radius allocations. */
export function allocationRoutes(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  grants: readonly PassiveAllocationGrant[] = [],
  budget = Infinity,
): Map<number, number[]> {
  const access = allocationAccess(graph, allocated, root, grants);
  const radiusNodes = new Set(grants.flatMap(grant => grant.nodes));
  const sources = new Set([...allocated].filter(id => !radiusNodes.has(id) || access.connected.has(id)));
  access.roots.forEach(id => sources.add(id));
  const paths = new Map<number, number[]>([...sources].filter(id => graph.has(id)).map(id => [id, []]));
  const queue = [...paths.keys()].sort((a, b) => a - b);
  const visited = new Set<number>();
  while (queue.length) {
    queue.sort((a, b) => paths.get(a)!.length - paths.get(b)!.length || a - b);
    const current = queue.shift()!;
    if (visited.has(current)) continue;
    visited.add(current);
    const path = paths.get(current)!;
    for (const next of graph.get(current) ?? []) {
      const candidate = allocated.has(next) ? path : [...path, next];
      if (candidate.length > budget || visited.has(next) || (paths.has(next) && paths.get(next)!.length <= candidate.length)) continue;
      paths.set(next, candidate); queue.push(next);
    }
  }
  for (const id of access.free) {
    if (!allocated.has(id) && budget >= 1 && (!paths.has(id) || paths.get(id)!.length > 1)) paths.set(id, [id]);
  }
  return paths;
}

export function shortestAllocationPath(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  target: number,
  grants: readonly PassiveAllocationGrant[] = [],
): number[] {
  if (allocated.has(target)) return [];
  return allocationRoutes(graph, allocated, root, grants).get(target) ?? (grants.length ? [] : [target]);
}

/** Include radius points without traversing out from their disconnected islands. */
export function connectedAllocation(
  graph: PassiveGraph,
  allocated: ReadonlySet<number>,
  root: number | null,
  grants: readonly PassiveAllocationGrant[] = [],
): Set<number> {
  if (root === null || !graph.has(root)) return new Set(allocated);
  const access = allocationAccess(graph, allocated, root, grants);
  return new Set([...access.connected, ...[...access.free].filter(id => allocated.has(id))]);
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
  grants: readonly PassiveAllocationGrant[] = [],
): Set<number> {
  const remaining = new Set(allocated);
  remaining.delete(removed);
  if (connectedAllocation(graph, allocated, root, grants).size !== allocated.size) return remaining;
  return connectedAllocation(graph, remaining, root, grants);
}

/** Revoke only allocations whose known jewel dependency was removed or changed. */
export function reconcileJewelAllocation(
  nodes: PassiveNode[], allocated: number[], className: string,
  before?: PassiveJewelState, after?: PassiveJewelState,
  remaining: number[] = allocated,
): number[] {
  if (!before?.allocation_grants.length || !after) return remaining;
  const root = before.class_starts[className.toLowerCase()] ?? classStartSkill(nodes, className);
  const graph = buildPassiveGraph(nodes);
  const previous = connectedAllocation(graph, new Set(allocated), root, before.allocation_grants);
  const reachable = connectedAllocation(graph, new Set(remaining), root, after.allocation_grants);
  // Unknown imported islands and ascendancy allocations are outside this model.
  return remaining.filter(id => !previous.has(id) || reachable.has(id));
}
