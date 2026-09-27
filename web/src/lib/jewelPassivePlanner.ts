import { getBackend, type JewelBase } from '../api/backend';
import type { AttributeChoice, CalculateBuildRequest, CalculateBuildResponse, JewelInput, PassiveJewelState, PassiveNode } from '../api/types';
import { normalizeCopiedItem, validateReplacement } from './itemReplacement';
import { COLLECT_STATS, compareObjectiveStats, evaluateVariants, feasibleOf, type EvaluateOptions, type EvaluateResult, type Objective } from './optimize';
import { allocationRoutes, connectedAllocation } from './passiveGraph';
import { allocationPaths, passivePlanningContext, refundableBranches, PASSIVE_POINT_LIMIT, type PassivePlan } from './passivePlanner';
import { loadTradeCatalog, type TradeCatalog } from './tradeOptimizer';

export const JEWEL_PASSIVE_CANDIDATE_LIMIT = 8;
/** Variant evaluations per jewel choice, including the unchanged-jewel choice. */
export const JEWEL_PASSIVE_PLAN_LIMIT = 512;
export interface JewelPassiveCandidate { id: string; label: string; jewel: JewelInput }
export interface JewelPassivePlan extends PassivePlan {
  candidateId: string | null;
  candidateLabel: string;
  jewels: JewelInput[];
  allocatedNodes: number[];
  attributeChoices: Record<string, AttributeChoice>;
  stats: Record<string, number>;
}
export interface JewelPassivePlannerResult {
  baseline: Record<string, number>;
  plans: JewelPassivePlan[];
  evaluated: number;
  limited: boolean;
  unmodeled: number;
  diagnostics: string[];
  aborted: boolean;
}
export interface JewelPassivePlannerOptions {
  request: CalculateBuildRequest;
  nodes: PassiveNode[];
  className?: string;
  ascendancy?: string;
  exclusiveNodes?: readonly number[];
  candidates: JewelPassiveCandidate[];
  points: number;
  mode: 'allocate' | 'reallocate';
  objective: Objective;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number) => void;
  calculateBuild?: (request: CalculateBuildRequest) => Promise<CalculateBuildResponse>;
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
  catalog?: TradeCatalog;
  jewelBases?: JewelBase[];
  translate?: (lines: string[]) => Promise<string[]>;
}
const key = (plan: PassivePlan) => `${[...plan.allocate].sort((a, b) => a - b)}/${[...plan.deallocate].sort((a, b) => a - b)}`;
const statsOf = (response: CalculateBuildResponse) => Object.fromEntries(response.stats
  .filter(stat => COLLECT_STATS.includes(stat.id) && stat.value !== null).map(stat => [stat.id, stat.value as number]));

/** Search each jewel against its own engine-derived topology, then verify exact complete states. */
export async function planJewelPassiveUpgrades(options: JewelPassivePlannerOptions): Promise<JewelPassivePlannerResult> {
  const { request, objective, signal } = options;
  const result: JewelPassivePlannerResult = { baseline: {}, plans: [], evaluated: 0, limited: false, unmodeled: 0, diagnostics: [], aborted: false };
  const cancel = () => { result.aborted = true; result.plans = []; return result; };
  if (signal?.aborted) return cancel();
  if (new Set(options.candidates.map(candidate => candidate.id)).size !== options.candidates.length) throw new Error('Candidate IDs must be unique');
  if (options.exclusiveNodes?.length) throw new Error('Weapon-exclusive passives are not supported by joint planning');
  const budget = Math.floor(options.points);
  if (!Number.isFinite(budget) || budget < 1 || budget > PASSIVE_POINT_LIMIT) throw new Error('Passive point budget must be between 1 and 8');
  const backend = !options.calculateBuild || !options.translate ? await getBackend() : undefined;
  const calculate = options.calculateBuild ?? backend!.calculateBuild.bind(backend);
  const evaluate = options.evaluate ?? evaluateVariants;
  const translate = options.translate ?? backend!.translateLines.bind(backend);
  let catalog = options.candidates.length ? options.catalog ?? await loadTradeCatalog() : undefined;
  if (catalog) {
    const bases = options.jewelBases ?? (options.catalog ? [] : await (backend ?? await getBackend()).loadJewelBases());
    // Unique-only bases belong in item validation, never in the craftable market search pool.
    catalog = { ...catalog, bases: [...catalog.bases, ...bases.filter(base => !catalog!.bases.some(existing => existing.name === base.name))
      .map(base => ({ name: base.name, category: 'jewel', level: base.drop_level, tags: base.tags, implicits: base.implicits }))] };
  }
  const before = await calculate(request);
  result.evaluated += 1;
  result.baseline = statsOf(before);
  if (!Object.values(result.baseline).every(Number.isFinite)) throw new Error('Current build calculation contains non-finite stats');
  if (signal?.aborted) return cancel();
  if (!before.tree_effects) throw new Error('Jewel topology is unavailable for this tree version');
  const baselineEffects = before.tree_effects;
  result.diagnostics.push(...before.unsupported_modifiers, ...baselineEffects.warnings);
  if (before.item_errors.length) throw new Error('Current build contains item calculation errors');
  const original = new Set(request.allocated_nodes ?? []);
  const currentJewels = request.jewels ?? [];
  const baselineUnsupported = new Set(before.unsupported_modifiers);
  const baselineWarnings = new Set(baselineEffects.warnings);
  const filled = currentJewels.map(jewel => jewel.socket_node);
  const makeContext = (allocated: number[], jewels: JewelInput[], effects: PassiveJewelState) => {
    // Unresolved seed nodes must never become proposed additions. Keep existing ones protected.
    const unresolved = new Set(effects.unresolved);
    const nodes = options.nodes.filter(node => !unresolved.has(node.skill) || original.has(node.skill));
    return passivePlanningContext(nodes, allocated, options.className, [], jewels.map(jewel => jewel.socket_node), options.ascendancy, effects);
  };
  const originalContext = makeContext([...original], currentJewels, baselineEffects);
  if (originalContext.root === null) throw new Error('Class root is unavailable for this tree version');
  if (options.mode === 'reallocate' && !originalContext.canRefund) throw new Error('Current passive allocation cannot be safely refunded');
  const previouslyConnected = connectedAllocation(originalContext.graph, originalContext.allocated, originalContext.root, baselineEffects.allocation_grants);
  const protectedOriginal = originalContext.protectedNodes;
  const choices: (JewelPassiveCandidate | null)[] = [null, ...options.candidates.slice(0, JEWEL_PASSIVE_CANDIDATE_LIMIT)];
  result.limited = options.candidates.length > JEWEL_PASSIVE_CANDIDATE_LIMIT;
  const total = choices.length * (JEWEL_PASSIVE_PLAN_LIMIT + 23) + 1;
  const report = () => options.onProgress?.(result.evaluated, total);
  const issue = (label: string, message: string) => { result.diagnostics.push(`${label}: ${message}`); };
  const acceptable = (response: CalculateBuildResponse, label: string) => {
    const unsupported = response.unsupported_modifiers.filter(line => !baselineUnsupported.has(line));
    const warnings = response.tree_effects?.warnings.filter(line => !baselineWarnings.has(line)) ?? [];
    if (unsupported.length || warnings.length) {
      result.unmodeled += 1;
      [...unsupported, ...warnings].forEach(line => issue(label, line));
      return false;
    }
    if (response.item_errors.length || !response.tree_effects) {
      issue(label, response.item_errors.length ? 'Item calculation failed' : 'Jewel topology unavailable');
      return false;
    }
    return true;
  };
  for (const choice of choices) {
    if (signal?.aborted) return cancel();
    const label = choice?.label ?? 'Current jewels';
    try {
      const socket = choice?.jewel.socket_node;
      let path: number[] = [];
      let jewels = currentJewels;
      if (choice) {
        if (options.nodes.find(node => node.skill === socket)?.kind !== 'jewel_socket' || !originalContext.byId.has(socket!)) {
          issue(label, 'Invalid main-tree jewel socket'); continue;
        }
        const route = allocationRoutes(originalContext.graph, originalContext.allocated, originalContext.root, originalContext.grants, budget).get(socket!);
        if (!route) { issue(label, 'Socket is outside the point budget'); continue; }
        path = route;
        const text = normalizeCopiedItem(choice.jewel.text);
        await validateReplacement({ ...request, allocated_nodes: [...original, ...path] }, `Jewel@${socket}`, text, catalog!, translate);
        jewels = [...currentJewels.filter(jewel => jewel.socket_node !== socket), { socket_node: socket!, text }];
        const uniqueName = (raw: string) => raw.match(/^Rarity:\s*UNIQUE\s*\n([^\n]+)/im)?.[1].trim();
        const limitLine = text.match(/^Limited to:\s*(.+)$/im)?.[1].trim();
        if (limitLine) {
          const limit = /^\d+$/.test(limitLine) ? Number(limitLine) : null;
          const name = uniqueName(text);
          if (limit === null || !name) { issue(label, 'Jewel limit cannot be verified'); continue; }
          if (jewels.filter(jewel => uniqueName(jewel.text) === name).length > limit) {
            issue(label, 'Jewel limit exceeded'); continue;
          }
        }
      }
      let seedRequest = { ...request, jewels, allocated_nodes: [...original, ...path] };
      let seedResponse = choice ? await calculate(seedRequest) : before;
      if (choice) result.evaluated += 1;
      if (signal?.aborted) return cancel();
      if (!acceptable(seedResponse, label)) continue;
      let context = makeContext(seedRequest.allocated_nodes, jewels, seedResponse.tree_effects!);
      const reachable = connectedAllocation(context.graph, context.allocated, context.root, context.grants);
      const forced = [...original].filter(id => previouslyConnected.has(id) && !reachable.has(id));
      if (forced.length && options.mode === 'allocate') { issue(label, 'Replacement would disconnect existing passive allocations'); continue; }
      if (forced.length > budget || forced.some(id => protectedOriginal.has(id) || context.protectedNodes.has(id))) {
        issue(label, 'Dependent refunds exceed the budget or include protected nodes'); continue;
      }
      if (forced.length) {
        seedRequest = { ...seedRequest, allocated_nodes: seedRequest.allocated_nodes.filter(id => !forced.includes(id)) };
        seedResponse = await calculate(seedRequest); result.evaluated += 1;
        if (signal?.aborted) return cancel();
        if (!acceptable(seedResponse, label)) continue;
        context = makeContext(seedRequest.allocated_nodes, jewels, seedResponse.tree_effects!);
      }
      // Every generated plan is expressed relative to the original build, including socket travel and forced refunds.
      const valid = (plan: PassivePlan, effects = seedResponse.tree_effects!): boolean => {
        const added = new Set(plan.allocate); const removed = new Set(plan.deallocate);
        if (added.size !== plan.allocate.length || removed.size !== plan.deallocate.length
          || [...added].some(id => original.has(id) || removed.has(id))
          || [...removed].some(id => !original.has(id) || protectedOriginal.has(id))) return false;
        if (options.mode === 'allocate' ? removed.size > 0 || added.size > budget : removed.size > budget || added.size > removed.size) return false;
        const final = [...original].filter(id => !removed.has(id)).concat([...added]);
        const finalSet = new Set(final);
        if (socket !== undefined && !finalSet.has(socket)) return false;
        if (filled.some(id => original.has(id) && !finalSet.has(id))) return false;
        const finalContext = makeContext(final, jewels, effects);
        if (finalContext.root === null || [...added].some(id => !finalContext.byId.has(id) || finalContext.roots.has(id)
          || effects.unresolved.includes(id) || baselineEffects.unresolved.includes(id))) return false;
        if (final.some(id => finalContext.byId.get(id)?.unlock_constraint?.nodes.some(required => !finalSet.has(required)))) return false;
        const connected = connectedAllocation(finalContext.graph, finalContext.allocated, finalContext.root, finalContext.grants);
        return final.every(id => (!previouslyConnected.has(id) && !added.has(id)) || connected.has(id));
      };
      const seed: PassivePlan = { allocate: path, deallocate: forced, target: socket ?? context.root ?? 0 };
      const probes = new Map<string, PassivePlan>();
      const add = (plan: PassivePlan) => { if (valid(plan)) probes.set(key(plan), plan); };
      add(seed);
      const refundBranches = options.mode === 'reallocate' ? refundableBranches(context, budget - forced.length)
        .filter(plan => !plan.deallocate.some(id => path.includes(id) || protectedOriginal.has(id))) : [];
      const refundSeeds = [[], ...refundBranches.slice(0, 12).map(plan => plan.deallocate)];
      result.limited ||= refundBranches.length > 12;
      for (const refund of refundSeeds) {
        const removed = [...new Set([...forced, ...refund])];
        const remainingBudget = (options.mode === 'allocate' ? budget : removed.length) - path.length;
        if (remainingBudget < 0) continue;
        const remaining = new Set(seedRequest.allocated_nodes.filter(id => !refund.includes(id)));
        add({ ...seed, deallocate: removed });
        const paths = allocationPaths(context, remaining, remainingBudget)
          .filter(plan => !plan.allocate.some(id => removed.includes(id) || baselineEffects.unresolved.includes(id)));
        const cap = refundSeeds.length > 1 ? 18 : 254;
        result.limited ||= paths.length > cap;
        for (const plan of paths.slice(0, cap)) add({ ...plan, allocate: [...new Set([...path, ...plan.allocate])], deallocate: removed });
      }
      const firstPlans = [...probes.values()].slice(0, 256);
      result.limited ||= probes.size > firstPlans.length;
      const run = async (plans: PassivePlan[]) => {
        const response = await evaluate({ request: seedRequest, variants: plans.map(plan => ({
          allocate_nodes: plan.allocate.filter(id => !path.includes(id)),
          deallocate_nodes: plan.deallocate.filter(id => !forced.includes(id)),
        })), signal, onProgress: done => options.onProgress?.(result.evaluated + done, total) });
        result.evaluated += response.results.length;
        const rows = response.results.flatMap(row => {
          if (row.error) { issue(label, row.error); return []; }
          const unsupported = row.unsupported?.filter(line => !baselineUnsupported.has(line)) ?? [];
          if (unsupported.length) { result.unmodeled += 1; unsupported.forEach(line => issue(label, line)); return []; }
          if (!Object.values(row.stats).every(Number.isFinite)) { issue(label, 'Candidate calculation contains non-finite stats'); return []; }
          return [{ plan: plans[row.index], stats: row.stats }];
        }).sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective));
        return { rows, aborted: response.aborted };
      };
      if (!firstPlans.length) { issue(label, 'No legal allocation within the point budget'); continue; }
      const first = await run(firstPlans);
      if (signal?.aborted || first.aborted) return cancel();
      const unions = new Map<string, PassivePlan>();
      const beam = first.rows.slice(0, 32);
      result.limited ||= first.rows.length > beam.length;
      for (let i = 0; i < beam.length; i += 1) for (let j = i + 1; j < beam.length; j += 1) {
        if (String(beam[i].plan.deallocate) !== String(beam[j].plan.deallocate)) continue;
        const plan = { ...beam[i].plan, allocate: [...new Set([...beam[i].plan.allocate, ...beam[j].plan.allocate])] };
        if (!probes.has(key(plan)) && valid(plan)) unions.set(key(plan), plan);
      }
      const secondPlans = [...unions.values()].slice(0, JEWEL_PASSIVE_PLAN_LIMIT - firstPlans.length);
      result.limited ||= unions.size > secondPlans.length;
      const second = secondPlans.length ? await run(secondPlans) : { rows: [], aborted: false };
      if (signal?.aborted || second.aborted) return cancel();
      const ranked = [...first.rows, ...second.rows].filter(row => feasibleOf(row.stats, objective)
        && compareObjectiveStats(row.stats, result.baseline, objective) < 0)
        .sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective));
      result.limited ||= ranked.length > 20;
      for (const { plan } of ranked.slice(0, 20)) {
        const allocatedNodes = [...original].filter(id => !plan.deallocate.includes(id)).concat(plan.allocate);
        const attributeChoices = Object.fromEntries(Object.entries(request.attribute_choices ?? {}).filter(([id]) => allocatedNodes.includes(Number(id))));
        const final = await calculate({ ...request, jewels, allocated_nodes: allocatedNodes, attribute_choices: attributeChoices });
        result.evaluated += 1;
        if (signal?.aborted) return cancel();
        if (!acceptable(final, label)) continue;
        if (!valid(plan, final.tree_effects!)) { issue(label, 'Final jewel topology does not support this allocation'); continue; }
        const stats = statsOf(final);
        if (!Object.values(stats).every(Number.isFinite)) { issue(label, 'Final calculation contains non-finite stats'); continue; }
        if (feasibleOf(stats, objective) && compareObjectiveStats(stats, result.baseline, objective) < 0) {
          result.plans.push({ ...plan, candidateId: choice?.id ?? null, candidateLabel: label, jewels, allocatedNodes, attributeChoices, stats });
        }
      }
      report();
    } catch (error) {
      if (signal?.aborted) return cancel();
      issue(label, error instanceof Error ? error.message : String(error));
    }
  }
  result.plans.sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective)
    || a.allocate.length + a.deallocate.length - b.allocate.length - b.deallocate.length);
  result.plans = result.plans.slice(0, 20);
  result.diagnostics = [...new Set(result.diagnostics)];
  options.onProgress?.(result.evaluated, result.evaluated);
  return result;
}
