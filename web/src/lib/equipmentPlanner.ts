import { getBackend, type PobrBackend } from '../api/backend';
import type { CalculateBuildRequest, SlotItemInput } from '../api/types';
import { validateReplacement } from './itemReplacement';
import { compareObjectiveStats, evaluateVariants, VARIANT_TOTAL_CAP, type EvaluateOptions, type EvaluateResult, type Objective } from './optimize';
import { applyEquippedAugmentLimits } from './replacementAugments';
import { loadTradeCatalog, type TradeCatalog } from './tradeOptimizer';

export const EQUIPMENT_CANDIDATE_LIMIT = 16;
export const EQUIPMENT_MAX_REPLACEMENTS = 10;
const BEAM_WIDTH = 64;

const EQUIPMENT_SLOTS = new Set([
  'weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots',
  'amulet', 'ring1', 'ring2', 'belt',
]);

/** A prepared, actual item. Distinct destinations of one pasted item share itemId. */
export interface EquipmentCandidate {
  id: string;
  itemId: string;
  label: string;
  slot: string;
  text: string;
}

export interface EquipmentPlan {
  candidateIds: string[];
  /** Only the replacement items; the caller retains all other equipped items. */
  items: SlotItemInput[];
  stats: Record<string, number>;
  unsupported: string[];
  warnings?: string[];
  /** A plan with an error must not be applied. */
  error?: string;
}

export interface EquipmentPlanningResult {
  baseline: Record<string, number>;
  plans: EquipmentPlan[];
  /** Number of complete variants sent for calculation, including the no-op. */
  evaluated: number;
  /** True when the search stopped before evaluating every legal combination. */
  limited?: boolean;
  maxReplacements?: number;
}

export interface EquipmentPlanningOptions {
  request: CalculateBuildRequest;
  candidates: EquipmentCandidate[];
  maxReplacements?: number;
  /** Used to select branches when the complete search exceeds the evaluation cap. */
  objective?: Objective;
  signal?: AbortSignal;
  onProgress?: EvaluateOptions['onProgress'];
}

export interface EquipmentPlanningDependencies {
  catalog?: TradeCatalog;
  translate?: PobrBackend['translateLines'];
  augmentInfo?: PobrBackend['itemAugmentInfo'];
  runeCatalog?: PobrBackend['runeCatalog'];
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
}

function abortIfRequested(signal?: AbortSignal): void {
  signal?.throwIfAborted();
}

function finalEquipment(request: CalculateBuildRequest, replacements: SlotItemInput[]): SlotItemInput[] {
  const slots = new Set(replacements.map(item => item.slot));
  return [...(request.items ?? []).filter(item => !slots.has(item.slot)), ...replacements];
}

function combinations(candidates: EquipmentCandidate[]): EquipmentCandidate[][] {
  const groups: EquipmentCandidate[][] = [[]];
  for (const candidate of candidates) groups.push([candidate]);
  for (let first = 0; first < candidates.length; first += 1) {
    for (let second = first + 1; second < candidates.length; second += 1) {
      const a = candidates[first], b = candidates[second];
      if (a.slot !== b.slot && a.itemId !== b.itemId) groups.push([a, b]);
    }
  }
  return groups;
}

function compatible(group: EquipmentCandidate[], candidate: EquipmentCandidate): boolean {
  return group.every(item => item.slot !== candidate.slot && item.itemId !== candidate.itemId);
}

function furtherCombinations(candidates: EquipmentCandidate[], maxSize: number, limit: number): EquipmentCandidate[][] {
  const groups: EquipmentCandidate[][] = [];
  const visit = (group: EquipmentCandidate[], next: number) => {
    if (group.length >= 3) groups.push(group);
    if (group.length === maxSize || groups.length > limit) return;
    for (let i = next; i < candidates.length && groups.length <= limit; i += 1) {
      if (compatible(group, candidates[i])) visit([...group, candidates[i]], i + 1);
    }
  };
  visit([], 0);
  return groups;
}

function maximumCompatibleSize(candidates: EquipmentCandidate[], maxSize: number): number {
  let best = 0;
  const visit = (group: EquipmentCandidate[], next: number) => {
    best = Math.max(best, group.length);
    if (best === maxSize || group.length + candidates.length - next <= best) return;
    for (let i = next; i < candidates.length && best < maxSize; i += 1) {
      if (compatible(group, candidates[i])) visit([...group, candidates[i]], i + 1);
    }
  };
  visit([], 0);
  return best;
}

/** Evaluate complete loadouts, preserving all singles and pairs before bounded deeper search. */
export async function planEquipmentUpgrades(
  options: EquipmentPlanningOptions,
  dependencies: EquipmentPlanningDependencies = {},
): Promise<EquipmentPlanningResult> {
  const { request, candidates, signal, onProgress } = options;
  const maxReplacements = options.maxReplacements ?? 2;
  if (!Number.isInteger(maxReplacements) || maxReplacements < 1 || maxReplacements > EQUIPMENT_MAX_REPLACEMENTS) {
    throw new Error(`Maximum equipment replacements must be 1–${EQUIPMENT_MAX_REPLACEMENTS}`);
  }
  if (candidates.length > EQUIPMENT_CANDIDATE_LIMIT) {
    throw new Error(`Too many equipment candidates (maximum ${EQUIPMENT_CANDIDATE_LIMIT})`);
  }
  if (new Set(candidates.map(candidate => candidate.id)).size !== candidates.length ||
    candidates.some(candidate => !candidate.id || !candidate.itemId)) {
    throw new Error('Equipment candidates require distinct IDs and an item ID');
  }
  abortIfRequested(signal);
  const catalog = dependencies.catalog ?? await loadTradeCatalog();
  const backend = !dependencies.translate || !dependencies.augmentInfo || !dependencies.runeCatalog
    ? await getBackend() : undefined;
  const translate = dependencies.translate ?? backend!.translateLines;
  const augmentInfo = dependencies.augmentInfo ?? backend!.itemAugmentInfo;
  const runeCatalog = dependencies.runeCatalog ?? backend!.runeCatalog;
  const evaluate = dependencies.evaluate ?? evaluateVariants;
  const runes = await runeCatalog();
  abortIfRequested(signal);

  const initial = new Map<string, { error?: string; info?: Awaited<ReturnType<typeof augmentInfo>> }>();
  for (const candidate of candidates) {
    abortIfRequested(signal);
    if (!EQUIPMENT_SLOTS.has(candidate.slot)) {
      initial.set(candidate.id, { error: 'wrong-slot' });
      continue;
    }
    try {
      await validateReplacement(request, candidate.slot, candidate.text, catalog, translate);
      abortIfRequested(signal);
      initial.set(candidate.id, { info: await augmentInfo(candidate.text) });
    } catch (error) {
      initial.set(candidate.id, { error: error instanceof Error ? error.message : String(error) });
    }
  }
  abortIfRequested(signal);

  const plans: EquipmentPlan[] = [];
  let evaluated = 0;
  let baseline: Record<string, number> = {};
  const addAndEvaluate = async (groups: EquipmentCandidate[][]): Promise<void> => {
    const calculated: number[] = [];
    for (const group of groups) {
      abortIfRequested(signal);
      const items = group.map(({ slot, text }) => ({ slot, text }));
      const plan: EquipmentPlan = { candidateIds: group.map(candidate => candidate.id), items, stats: {}, unsupported: [] };
      plans.push(plan);
      const failed = group.map(candidate => initial.get(candidate.id)?.error).find(Boolean);
      if (failed) { plan.error = failed; continue; }
      if (group.length) {
        const finalItems = finalEquipment(request, items);
        const finalRequest = { ...request, items: finalItems };
        try {
          // Validate the completed loadout, including cross-item augment limits.
          for (const candidate of group) {
            await validateReplacement(finalRequest, candidate.slot, candidate.text, catalog, translate);
            abortIfRequested(signal);
            const info = initial.get(candidate.id)!.info!;
            const limits = applyEquippedAugmentLimits(info, { catalog: runes, items: finalItems, slot: candidate.slot });
            if (limits.exceeds(info.runes)) throw new Error('augment-limit');
            const known = new Set([...runes, ...info.options].flatMap(entry =>
              [entry.name, entry.name_zh_cn, entry.name_zh_tw].filter((name): name is string => !!name)));
            if (limits.limitWarning || info.reason === 'unknown_augments' || info.runes.some(name => name && !known.has(name))) {
              plan.warnings = ['unknown-equipped-augments'];
            }
          }
          if (group.some(candidate => candidate.slot === 'weapon1')) {
            const offhand = finalItems.find(item => item.slot === 'weapon2');
            if (offhand) await validateReplacement(finalRequest, offhand.slot, offhand.text, catalog, translate);
          }
        } catch (error) {
          plan.error = error instanceof Error ? error.message : String(error);
        }
      }
      if (!plan.error) calculated.push(plans.length - 1);
    }
    if (!calculated.length) return;
    if (evaluated + calculated.length > VARIANT_TOTAL_CAP) throw new Error('equipment-evaluation-limit');
    const variants = calculated.map(index => ({ set_items: plans[index].items }));
    const previous = evaluated;
    const result = await evaluate({ request, variants, signal,
      onProgress: (done, total) => onProgress?.(previous + done, previous + total) });
    if (result.aborted || signal?.aborted) throw new DOMException('Equipment planning cancelled', 'AbortError');
    if (previous === 0) {
      if (Object.values(result.baseline).some(value => !Number.isFinite(value))) throw new Error('non-finite-baseline');
      baseline = result.baseline;
    }
    const rows = new Map(result.results.map(row => [row.index, row]));
    calculated.forEach((planIndex, variantIndex) => {
      const plan = plans[planIndex];
      const row = rows.get(variantIndex);
      if (!row) { plan.error = 'missing-result'; return; }
      if (row.error) { plan.error = row.error; return; }
      if (Object.values(row.stats).some(value => !Number.isFinite(value))) { plan.error = 'non-finite-stats'; return; }
      plan.stats = row.stats;
      plan.unsupported = row.unsupported ?? [];
      if (plan.warnings?.length) plan.error = plan.warnings[0];
    });
    evaluated += variants.length;
  };

  const firstGroups = combinations(candidates).filter(group => group.length <= maxReplacements);
  await addAndEvaluate(firstGroups);
  let limited = false;
  if (maxReplacements >= 3) {
    const remaining = VARIANT_TOTAL_CAP - evaluated;
    const searchCandidates = candidates.filter(candidate => !initial.get(candidate.id)?.error);
    const exhaustive = furtherCombinations(searchCandidates, maxReplacements, remaining);
    if (exhaustive.length <= remaining) {
      await addAndEvaluate(exhaustive);
    } else {
      limited = true;
      const objective = options.objective ?? { stat: 'TotalDPS', constraints: [] };
      const byId = new Map(candidates.map((candidate, index) => [candidate.id, index]));
      const reachableMax = maximumCompatibleSize(searchCandidates, maxReplacements);
      // A currently invalid augment combination can become legal when another
      // equipped item is replaced, so keep it available for expansion.
      let frontier = plans.filter(plan => plan.candidateIds.length === 2
        && plan.candidateIds.every(id => !initial.get(id)?.error));
      const seen = new Set(plans.map(plan => plan.candidateIds.join('\0')));
      for (let size = 3; size <= reachableMax && evaluated < VARIANT_TOTAL_CAP && frontier.length; size += 1) {
        const ordered = [...frontier].sort((a, b) => Number(!!a.error || !!a.unsupported.length) - Number(!!b.error || !!b.unsupported.length)
          || compareObjectiveStats(a.stats, b.stats, objective));
        const selected = ordered.length <= BEAM_WIDTH ? ordered : [
          ...ordered.slice(0, BEAM_WIDTH / 2),
          ...Array.from({ length: BEAM_WIDTH / 2 }, (_, index) =>
            ordered[Math.floor(BEAM_WIDTH / 2 + index * (ordered.length - BEAM_WIDTH / 2) / (BEAM_WIDTH / 2))]),
        ];
        const expansions = selected.map(plan => {
          const group = plan.candidateIds.map(id => candidates[byId.get(id)!]);
          return searchCandidates.filter(candidate => compatible(group, candidate))
            .map(candidate => [...group, candidate].sort((a, b) => byId.get(a.id)! - byId.get(b.id)!));
        });
        const next: EquipmentCandidate[][] = [];
        // Keep enough evaluations for every reachable depth; unused quota rolls
        // forward when a depth has fewer valid plans than anticipated.
        const quota = Math.max(1, Math.floor((VARIANT_TOTAL_CAP - evaluated) / (reachableMax - size + 1)));
        for (let round = 0; next.length < quota && expansions.some(group => group.length > round); round += 1) {
          for (const group of expansions) {
            const choice = group[round];
            if (!choice) continue;
            const key = choice.map(candidate => candidate.id).join('\0');
            if (!seen.has(key)) { seen.add(key); next.push(choice); }
            if (next.length >= quota) break;
          }
        }
        if (!next.length) break;
        const start = plans.length;
        await addAndEvaluate(next);
        frontier = plans.slice(start);
      }
    }
  }
  const noop = plans[0];
  if (noop.error) throw new Error(noop.error);
  noop.stats = baseline;
  return { baseline, plans, evaluated, limited, maxReplacements };
}
