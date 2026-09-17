import type { AttributeChoice, CalculateBuildRequest, PassiveNode, PassiveAllocationGrant, PassiveJewelState, VariantInput, VariantStats } from '../api/types';
import {
  compareObjectiveStats,
  evaluateVariants,
  feasibleOf,
  type EvaluateOptions,
  type EvaluateResult,
  type Objective,
} from './optimize';
import { allocationRoutes, buildPassiveGraph, classStartSkill, connectedAllocation, type PassiveGraph } from './passiveGraph';

const CLASS_START_NAMES = new Set(['TEMPLAR', 'DUELIST', 'RANGER', 'MARAUDER', 'SIX', 'WITCH']);
export const PASSIVE_PLAN_LIMIT = 512;
export const PASSIVE_POINT_LIMIT = 8;
const PROBE_LIMIT = 256;

export interface PassivePlan {
  allocate: number[];
  deallocate: number[];
  target: number;
}

export interface PassivePlanningContext {
  graph: PassiveGraph;
  byId: Map<number, PassiveNode>;
  allocated: Set<number>;
  root: number | null;
  grants: readonly PassiveAllocationGrant[];
  roots: Set<number>;
  canRefund: boolean;
  protectedNodes: Set<number>;
}

/** Plan common main-tree nodes only. Ascendancy points have a separate budget. */
export function passivePlanningContext(
  nodes: PassiveNode[],
  allocated: readonly number[],
  className: string | undefined,
  exclusiveNodes: readonly number[] = [],
  filledSockets: readonly number[] = [],
  ascendancy?: string,
  treeEffects?: PassiveJewelState,
): PassivePlanningContext {
  const root = treeEffects?.class_starts[className?.toLowerCase() ?? ''] ?? classStartSkill(nodes, className);
  const grants = treeEffects?.allocation_grants ?? [];
  const classStarts = new Set(Object.values(treeEffects?.class_starts ?? {}));
  const roots = new Set([...(root === null ? [] : [root]), ...grants.flatMap(grant => grant.roots)]);
  const exclusive = new Set(exclusiveNodes);
  const allAllocated = new Set(allocated);
  const eligible = nodes.filter(node => !node.ascendancy_id && node.kind !== 'mastery'
    && !exclusive.has(node.skill)
    && (!(classStarts.has(node.skill) || CLASS_START_NAMES.has(node.name ?? '')) || roots.has(node.skill))
    && (!node.unlock_constraint || ((!node.unlock_constraint.ascendancy || node.unlock_constraint.ascendancy === ascendancy)
      && node.unlock_constraint.nodes.every(id => allAllocated.has(id) && !exclusive.has(id)))));
  const allNodes = new Map(nodes.map(node => [node.skill, node]));
  const byId = new Map(eligible.map(node => [node.skill, node]));
  const graph = buildPassiveGraph(eligible);
  const current = new Set(allocated.filter(id => byId.has(id)));
  const rooted = root !== null && connectedAllocation(graph, current, root, grants).size === current.size;
  const omittedMainNodes = allocated.some(id => !byId.has(id) && !allNodes.get(id)?.ascendancy_id);
  return {
    graph, byId, allocated: current, root, grants, roots,
    // Imports with unexplained topology remain extendable but must never be pruned automatically.
    canRefund: rooted && exclusive.size === 0 && !omittedMainNodes,
    protectedNodes: new Set([...roots, ...filledSockets, ...(treeEffects?.unresolved ?? []),
      ...nodes.filter(node => allAllocated.has(node.skill)).flatMap(node => node.unlock_constraint?.nodes ?? [])]),
  };
}

/** Prefill only unallocated travel nodes; active weapon-set attributes must keep their current choice. */
export function withTravelAttributes(
  request: CalculateBuildRequest,
  nodes: PassiveNode[],
  choice: AttributeChoice,
): CalculateBuildRequest {
  const allocated = new Set(request.allocated_nodes ?? []);
  const choices = { ...request.attribute_choices };
  for (const node of nodes) {
    if (!allocated.has(node.skill) && node.name === 'Attribute') choices[String(node.skill)] = choice;
  }
  return { ...request, attribute_choices: choices };
}

/** One BFS visits the neighborhood once; every returned path includes its travel-point cost. */
export function allocationPaths(
  context: PassivePlanningContext,
  allocated: ReadonlySet<number>,
  budget: number,
): PassivePlan[] {
  const eligible = (id: number) => !context.byId.get(id)?.unlock_constraint?.nodes
    .some(required => context.allocated.has(required) && !allocated.has(required));
  const graph = new Map([...context.graph].filter(([id]) => eligible(id))
    .map(([id, neighbours]) => [id, neighbours.filter(eligible)]));
  const routes = allocationRoutes(graph, allocated, context.root, context.grants, budget);
  const out: PassivePlan[] = [];
  for (const [target, allocate] of routes) {
    if (allocate.length === 0 || allocated.has(target) || context.roots.has(target)) continue;
    const node = context.byId.get(target)!;
    if (node.kind !== 'jewel_socket' || context.protectedNodes.has(target)) out.push({ allocate, deallocate: [], target });
  }
  return out.sort((a, b) => a.allocate.length - b.allocate.length
    || Number(context.byId.get(b.target)?.kind === 'notable') - Number(context.byId.get(a.target)?.kind === 'notable')
    || a.target - b.target);
}

/** Remove only whole terminal branches, retaining every other allocated node's root connection. */
export function refundableBranches(context: PassivePlanningContext, budget: number): PassivePlan[] {
  if (!context.canRefund) return [];
  const out = new Map<string, PassivePlan>();
  for (const id of [...context.allocated].sort((a, b) => a - b)) {
    if (context.protectedNodes.has(id)) continue;
    const remaining = new Set(context.allocated);
    remaining.delete(id);
    const connected = connectedAllocation(context.graph, remaining, context.root, context.grants);
    const removed = [...context.allocated].filter(node => !connected.has(node)).sort((a, b) => a - b);
    if (removed.length === 0 || removed.length > budget || removed.some(node => context.protectedNodes.has(node))) continue;
    out.set(removed.join(','), { allocate: [], deallocate: removed, target: id });
  }
  return [...out.values()].sort((a, b) => a.deallocate.length - b.deallocate.length || a.target - b.target);
}

function planKey(plan: PassivePlan): string {
  return `${[...plan.allocate].sort((a, b) => a - b).join(',')}/${plan.deallocate.join(',')}`;
}

function variantOf(plan: PassivePlan): VariantInput {
  return { allocate_nodes: plan.allocate, deallocate_nodes: plan.deallocate };
}

export interface PassivePlanResult extends PassivePlan {
  stats: Record<string, number>;
}

export interface PassivePlannerResult {
  baseline: Record<string, number>;
  plans: PassivePlanResult[];
  evaluated: number;
  limited: boolean;
  unmodeled: number;
}

/** Validate the exact evaluated edit before applying it to its original request. */
export function passivePlanAllocation(
  request: CalculateBuildRequest,
  context: PassivePlanningContext,
  plan: PassivePlan,
  points: number,
  mode: 'allocate' | 'reallocate',
): { allocatedNodes: number[]; attributeChoices: Record<string, AttributeChoice> } | null {
  const current = new Set(request.allocated_nodes ?? []);
  const removed = new Set(plan.deallocate);
  const added = new Set(plan.allocate);
  const budget = Math.min(PASSIVE_POINT_LIMIT, Math.floor(points));
  if (!Number.isFinite(budget) || budget < 1 || context.root === null
    || removed.size !== plan.deallocate.length || added.size !== plan.allocate.length
    || [...current].filter(id => context.byId.has(id)).some(id => !context.allocated.has(id))
    || [...context.allocated].some(id => !current.has(id))
    || [...removed].some(id => !context.allocated.has(id) || context.protectedNodes.has(id))
    || [...added].some(id => current.has(id) || removed.has(id) || !context.byId.has(id) || context.roots.has(id))) return null;
  if (mode === 'allocate' ? removed.size > 0 || added.size > budget
    : !context.canRefund || removed.size === 0 || removed.size > budget || added.size > removed.size) return null;
  const allocatedNodes = [...current].filter(id => !removed.has(id)).concat([...added]);
  const final = new Set(allocatedNodes);
  if ([...added].some(id => context.byId.get(id)?.unlock_constraint?.nodes.some(required => !final.has(required)))) return null;
  const main = new Set(allocatedNodes.filter(id => context.byId.has(id)));
  if (mode === 'reallocate' && connectedAllocation(context.graph, main, context.root, context.grants).size !== main.size) return null;
  // Unexplained imported components may be extended, but a new disconnected island is never legal.
  const retained = new Set([...context.allocated].filter(id => !removed.has(id)));
  const permitted = (id: number) => main.has(id) || context.roots.has(id);
  const finalGraph = new Map([...context.graph].filter(([id]) => permitted(id))
    .map(([id, neighbours]) => [id, neighbours.filter(permitted)]));
  const reachable = allocationRoutes(finalGraph, retained, context.root, context.grants, added.size);
  if ([...added].some(id => !reachable.has(id))) return null;
  return { allocatedNodes, attributeChoices: Object.fromEntries(Object.entries(request.attribute_choices ?? {})
    .filter(([id]) => final.has(Number(id)))) };
}

interface PlannerOptions {
  request: CalculateBuildRequest;
  context: PassivePlanningContext;
  points: number;
  mode: 'allocate' | 'reallocate';
  objective: Objective;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number) => void;
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
}

/** Bounded full-build search: connected paths, then interacting path unions or legal branch swaps. */
export async function planPassiveUpgrades(options: PlannerOptions): Promise<PassivePlannerResult> {
  const { context, objective, request, signal } = options;
  if (signal?.aborted) return { baseline: {}, plans: [], evaluated: 0, limited: false, unmodeled: 0 };
  const budget = Number.isFinite(options.points) ? Math.max(1, Math.min(PASSIVE_POINT_LIMIT, Math.floor(options.points))) : 1;
  const evaluate = options.evaluate ?? evaluateVariants;
  const paths = allocationPaths(context, context.allocated, budget);
  const refunds = options.mode === 'reallocate' ? refundableBranches(context, budget) : [];
  if (options.mode === 'reallocate' && refunds.length === 0) {
    return { baseline: {}, plans: [], evaluated: 0, limited: false, unmodeled: 0 };
  }
  const pathCap = options.mode === 'reallocate' ? PROBE_LIMIT - 65 : PROBE_LIMIT - 1;
  const probes = [{ allocate: [], deallocate: [], target: context.root ?? 0 }, ...paths.slice(0, pathCap), ...refunds.slice(0, 64)];
  if (probes.length === 1) return { baseline: {}, plans: [], evaluated: 0, limited: false, unmodeled: 0 };
  let count = 0;
  let limited = paths.length > pathCap || refunds.length > 64;
  const run = async (plans: PassivePlan[]) => {
    const result = await evaluate({ request, variants: plans.map(variantOf), signal,
      onProgress: done => { if (!signal?.aborted) options.onProgress?.(count + done, PASSIVE_PLAN_LIMIT); } });
    count += result.results.length;
    return result;
  };
  const first = await run(probes);
  if (signal?.aborted || first.aborted) return { baseline: first.baseline, plans: [], evaluated: count, limited, unmodeled: 0 };
  const identity = first.results.find(value => value.index === 0);
  if (!identity || identity.error) throw new Error(identity?.error ?? 'Passive baseline calculation is unavailable');
  const baselineUnsupported = new Set(identity.unsupported ?? []);
  const hasNewUnsupported = (value: VariantStats) => value.unsupported?.some(line => !baselineUnsupported.has(line)) ?? false;
  const good = (values: VariantStats[]) => values.filter(value => !value.error && !hasNewUnsupported(value) && Object.values(value.stats).every(Number.isFinite));
  const sorted = good(first.results).sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective));
  const rankedPaths = sorted.filter(value => probes[value.index].allocate.length > 0);
  const next = new Map<string, PassivePlan>();
  const seen = new Set(probes.map(planKey));
  const add = (plan: PassivePlan) => {
    const key = planKey(plan);
    if (!seen.has(key) && !next.has(key)) next.set(key, plan);
  };
  if (options.mode === 'allocate') {
    // Combine promising complete paths, keeping shared travel nodes only once.
    const beam = rankedPaths.slice(0, 24).map(value => probes[value.index]);
    for (let i = 0; i < beam.length; i += 1) {
      for (let j = i + 1; j < beam.length; j += 1) {
        const allocate = [...new Set([...beam[i].allocate, ...beam[j].allocate])];
        if (allocate.length <= budget) add({ allocate, deallocate: [], target: beam[i].target });
      }
    }
    for (let i = 0; i < Math.min(8, beam.length); i += 1) {
      for (const union of [...next.values()].slice(0, 32)) {
        const allocate = [...new Set([...beam[i].allocate, ...union.allocate])];
        if (allocate.length <= budget) add({ allocate, deallocate: [], target: beam[i].target });
      }
    }
  } else {
    const pathRank = new Map(rankedPaths.map((value, index) => [probes[value.index].target, index]));
    const rankedRefunds = sorted.filter(value => probes[value.index].deallocate.length > 0).slice(0, 12);
    for (const value of rankedRefunds) {
      const removed = probes[value.index].deallocate;
      const remaining = new Set([...context.allocated].filter(id => !removed.includes(id)));
      // Recompute paths after refunding: an attractive route may have used the removed branch.
      const replacements = allocationPaths(context, remaining, removed.length)
        .filter(plan => !plan.allocate.some(id => removed.includes(id)))
        .sort((a, b) => (pathRank.get(a.target) ?? Infinity) - (pathRank.get(b.target) ?? Infinity)
          || a.allocate.length - b.allocate.length || a.target - b.target);
      for (const plan of replacements.slice(0, 32)) add({ ...plan, deallocate: removed });
    }
  }
  const remainingCapacity = PASSIVE_PLAN_LIMIT - count;
  limited ||= next.size > remainingCapacity;
  const combinations = [...next.values()].slice(0, remainingCapacity);
  const second = combinations.length > 0 ? await run(combinations) : null;
  if (signal?.aborted || second?.aborted) return { baseline: first.baseline, plans: [], evaluated: count, limited, unmodeled: 0 };
  const candidates = [
    ...(options.mode === 'allocate' ? good(first.results).map(value => ({ ...probes[value.index], stats: value.stats })) : []),
    ...good(second?.results ?? []).map(value => ({ ...combinations[value.index], stats: value.stats })),
  ];
  const plans = candidates.filter(plan => feasibleOf(plan.stats, objective)
    && compareObjectiveStats(plan.stats, first.baseline, objective) < 0)
    .sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective)
      || (a.allocate.length - a.deallocate.length) - (b.allocate.length - b.deallocate.length));
  options.onProgress?.(count, count);
  return { baseline: first.baseline, plans: plans.slice(0, 10), evaluated: count, limited,
    unmodeled: [...first.results, ...(second?.results ?? [])].filter(hasNewUnsupported).length };
}
