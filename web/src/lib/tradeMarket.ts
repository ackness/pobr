import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, feasibleOf, scoreOf, type EvaluateOptions, type Objective } from './optimize';
import type { TradeCatalog, TradeGem } from './tradeOptimizer';
import { eligibleSupports, lineageAvailable, sameSupportFamily, supportSetsCompatible, type SupportMetadata } from './supportOptimizer';
import { preserveSkillSelection } from './skillSelection';

export type GemAcquisition = 'skill-adjustment' | 'market-lineage' | 'market-skill';
export function gemAcquisition(gem: TradeGem): GemAcquisition {
  return !gem.is_support ? 'market-skill' : gem.is_lineage ? 'market-lineage' : 'skill-adjustment';
}
export interface GemPlan { acquisition: GemAcquisition; gem: TradeGem; group: number; position: number; level: number; quality: number; variant: VariantInput; gainPercent?: number; gain?: number; stats?: Record<string, number>; baseline?: Record<string, number> }
/** Only propose known usable levels; old catalogs can still suggest quality upgrades. */
export function usableGemLevel(gem: TradeGem, characterLevel: number): number {
  return (gem.level_requirements ?? []).reduce((best, required, index) =>
    required <= characterLevel ? index + 1 : best, 0);
}
export function gemVariant(request: CalculateBuildRequest, group: number, position: number,
  gem: TradeGem, level: number, quality: number): VariantInput {
  return { socket_groups: (request.socket_groups ?? []).map((entry, index) => {
    if (index !== group) return entry;
    const gems = [...entry.gems];
    gems[position] = { ...(gems[position]?.skill_id === gem.skill_id ? gems[position] : {}),
      skill_id: gem.skill_id, level, quality };
    return { ...entry, gems };
  }) };
}

/** Compare complete single-gem replacements. Ordinary supports are skill adjustments;
 * only active and lineage gems produce market purchase plans.
 */
export async function planGemUpgrades(request: CalculateBuildRequest, catalog: TradeCatalog, group: number,
  objective: Objective, signal?: AbortSignal, onProgress?: EvaluateOptions['onProgress'],
  evaluate: typeof evaluateVariants = evaluateVariants): Promise<GemPlan[]> {
  const current = request.socket_groups?.[group];
  if (!current?.enabled || current.source || !current.gems.length) return [];
  const backend = await getBackend();
  const effectCatalog = await backend.gemCatalog();
  signal?.throwIfAborted();
  const effects = new Map(effectCatalog.map(gem => [gem.skill_id, gem.active_effect_ids]));
  const gems: SupportMetadata[] = (catalog.gems ?? []).flatMap(gem => effects.has(gem.skill_id)
    ? [{ ...gem, active_effect_ids: effects.get(gem.skill_id)! }] : []);
  const byId = new Map(gems.map(gem => [gem.skill_id, gem]));
  const plans: GemPlan[] = [];
  const supports = current.gems.flatMap((gem, index) => byId.get(gem.skill_id)?.is_support ? [index] : []);
  // Imported groups expose occupied sockets, not the unlocked capacity. Only the two
  // initial support sockets are assumed; additional sockets are configured in Skills.
  const capacity = Math.max(2, Math.min(5, supports.length));
  const characterLevel = request.character?.level ?? 1;
  const add = (gem: SupportMetadata, position: number, level: number, quality: number) => {
    const variant = gemVariant(request, group, position, gem, level, quality);
    const replacement = preserveSkillSelection(current, variant.socket_groups![group], byId);
    if (!replacement) return;
    variant.socket_groups![group] = replacement;
    const supportInputs = replacement.gems.filter(input => byId.get(input.skill_id)?.is_support);
    if (gem.is_support && supportInputs.some(input => !lineageAvailable(byId.get(input.skill_id)!, request.socket_groups ?? [], group))) return;
    plans.push({ acquisition: gemAcquisition(gem), gem, group, position, level, quality, variant });
  };
  for (const gem of eligibleSupports(current, gems, characterLevel).gems) {
    if (!lineageAvailable(gem, request.socket_groups ?? [], group)) continue;
    const level = Math.min(gem.max_level, usableGemLevel(gem, characterLevel));
    const positions = [...supports, ...(supports.length < capacity ? [current.gems.length] : [])];
    for (const position of positions) {
      if (current.gems[position]?.skill_id === gem.skill_id) continue;
      if (supports.some(index => index !== position && sameSupportFamily(byId.get(current.gems[index].skill_id)!, gem))) continue;
      add(gem, position, level, 0);
    }
  }
  current.gems.forEach((entry, position) => {
    const gem = byId.get(entry.skill_id);
    if (!gem) return;
    if (entry.quality < 20) add(gem, position, entry.level, 20);
    const level = Math.min(gem.max_level + 1, usableGemLevel(gem, characterLevel));
    if (!gem.is_support && entry.level < level) add(gem, position, level, Math.max(entry.quality, 20));
  });
  if (!plans.length) return [];
  const compatible = await supportSetsCompatible(current, plans.map(plan => plan.variant.socket_groups![group].gems
    .filter(input => byId.get(input.skill_id)?.is_support)), gems, signal);
  signal?.throwIfAborted();
  // Active-gem quality/level edits do not change the support type set.
  const legal = plans.filter((plan, index) => !plan.gem.is_support || compatible[index]);
  plans.splice(0, plans.length, ...legal);
  if (!plans.length) return [];
  const identity = await evaluate({ request, variants: [{}], signal });
  if (identity.aborted) throw new DOMException('Search cancelled', 'AbortError');
  signal?.throwIfAborted();
  if (identity.results[0]?.error) throw new Error(identity.results[0].error);
  const unsupported = new Set(identity.results[0]?.unsupported ?? []);
  const ranked: GemPlan[] = [];
  for (let start = 0; start < plans.length; start += 512) {
    signal?.throwIfAborted();
    const selected = plans.slice(start, start + 512);
    const result = await evaluate({ request, variants: selected.map(plan => plan.variant), signal,
      onProgress: done => onProgress?.(start + done, plans.length) });
    if (result.aborted) throw new DOMException('Search cancelled', 'AbortError');
    for (const row of result.results) {
      if (row.error || row.unsupported?.some(line => !unsupported.has(line))) continue;
      const gain = scoreOf(row.stats, objective) - scoreOf(result.baseline, objective);
      if (!Number.isFinite(gain) || !feasibleOf(row.stats, objective)
        || compareObjectiveStats(row.stats, result.baseline, objective) >= 0) continue;
      ranked.push({ ...selected[row.index], gain, stats: row.stats, baseline: result.baseline,
        gainPercent: gain / Math.max(Math.abs(scoreOf(result.baseline, objective)), 1) * 100 });
    }
  }
  signal?.throwIfAborted();
  const unique = new Map<string, GemPlan>();
  for (const plan of ranked.sort((a, b) => compareObjectiveStats(a.stats!, b.stats!, objective))) {
    const key = `${plan.gem.skill_id}:${plan.level}:${plan.quality}`;
    if (!unique.has(key)) unique.set(key, plan);
  }
  // Keep both acquisition paths visible even when cheap skill adjustments dominate.
  const counts = new Map<GemAcquisition, number>();
  return [...unique.values()].filter(plan => {
    const count = counts.get(plan.acquisition) ?? 0;
    counts.set(plan.acquisition, count + 1);
    return count < 3;
  });
}
