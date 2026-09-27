import { getBackend, type PobrBackend } from '../api/backend';
import type { CalculateBuildRequest, SlotItemInput } from '../api/types';
import { validateReplacement } from './itemReplacement';
import { evaluateVariants, type EvaluateOptions, type EvaluateResult } from './optimize';
import { applyEquippedAugmentLimits } from './replacementAugments';
import { loadTradeCatalog, type TradeCatalog } from './tradeOptimizer';

export const EQUIPMENT_CANDIDATE_LIMIT = 16;

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
}

export interface EquipmentPlanningOptions {
  request: CalculateBuildRequest;
  candidates: EquipmentCandidate[];
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

/** Evaluate every legal one- or two-item choice, including singles with a loss. */
export async function planEquipmentUpgrades(
  options: EquipmentPlanningOptions,
  dependencies: EquipmentPlanningDependencies = {},
): Promise<EquipmentPlanningResult> {
  const { request, candidates, signal, onProgress } = options;
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
  const calculated: number[] = [];
  for (const group of combinations(candidates)) {
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
        // The initial validation checks the original weapon family; this pass
        // checks the completed loadout, including a changed other weapon slot.
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
        // Changing an empty main hand can also invalidate an unchanged offhand.
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

  // There are at most 1 + 16 + C(16, 2) = 137 variants, below the shared cap.
  const variants = calculated.map(index => ({ set_items: plans[index].items }));
  const result = await evaluate({ request, variants, signal, onProgress });
  if (result.aborted || signal?.aborted) throw new DOMException('Equipment planning cancelled', 'AbortError');
  if (Object.values(result.baseline).some(value => !Number.isFinite(value))) {
    throw new Error('non-finite-baseline');
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
  const baseline = result.baseline;
  const noop = plans[0];
  if (noop.error) throw new Error(noop.error);
  noop.stats = baseline;
  return { baseline, plans, evaluated: variants.length };
}
