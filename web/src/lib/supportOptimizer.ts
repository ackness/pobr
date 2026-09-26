import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, GemInput, SocketGroupInput, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, feasibleOf, scoreOf, type EvaluateOptions, type Objective } from './optimize';
import type { TradeGem } from './tradeOptimizer';

/** Exact postfix expressions and type additions come from the pinned PoB catalog. */
export interface SupportMetadata extends TradeGem {
  is_lineage?: boolean;
  compatibility_known?: boolean;
  skill_types?: string[];
  require_skill_types?: string[];
  exclude_skill_types?: string[];
  add_skill_types?: string[];
  support_gems_only?: boolean;
  cannot_be_supported?: boolean;
  families?: string[];
}

export function usableSupportLevel(gem: TradeGem, characterLevel: number): number {
  return (gem.level_requirements ?? []).reduce((best, required, index) =>
    Number.isFinite(required) && required <= characterLevel && index < gem.max_level ? index + 1 : best, 0);
}

function known(gem: SupportMetadata | undefined): gem is SupportMetadata {
  return gem?.compatibility_known === true;
}

export function sameSupportFamily(a: SupportMetadata, b: SupportMetadata): boolean {
  const aFamilies = a.families?.length ? a.families : [a.family];
  const bFamilies = b.families?.length ? b.families : [b.family];
  return a.skill_id === b.skill_id || aFamilies.some(family => bFamilies.includes(family));
}

/** Local candidate policy only; the engine owns skill-type compatibility. */
function supportSetAllowed(gems: GemInput[], catalog: SupportMetadata[]): boolean {
  const entries = gems.map(input => catalog.find(gem => gem.skill_id === input.skill_id));
  if (entries.some(gem => !known(gem) || !gem.is_support)) return false;
  return !entries.some((gem, index) => entries.slice(0, index).some(other => sameSupportFamily(gem!, other!)));
}

/** Batch the authoritative group judgement before scoring, preserving candidate order. */
export async function supportSetsCompatible(group: SocketGroupInput, sets: GemInput[][],
  catalog: SupportMetadata[]): Promise<boolean[]> {
  const result = sets.map(() => false);
  const eligible = sets.flatMap((gems, index) => supportSetAllowed(gems, catalog) ? [{ gems, index }] : []);
  if (!eligible.length) return result;
  const active = group.gems.filter(input => !catalog.find(gem => gem.skill_id === input.skill_id)?.is_support);
  const backend = await getBackend();
  for (let start = 0; start < eligible.length; start += 512) {
    const batch = eligible.slice(start, start + 512);
    const compatible = await backend.supportGroupsCompatible(batch.map(({ gems }) => ({ ...group, gems: [...active, ...gems] })));
    if (compatible.length !== batch.length) throw new Error('Invalid support compatibility response.');
    batch.forEach(({ index }, i) => { result[index] = compatible[i]; });
  }
  return result;
}

export async function supportSetCompatible(group: SocketGroupInput, gems: GemInput[],
  catalog: SupportMetadata[]): Promise<boolean> {
  return (await supportSetsCompatible(group, [gems], catalog))[0];
}

/** Without a modeled copy-limit modifier, keep the game's default of one copy.
 * Include inactive weapon groups conservatively because materialized requests disable them.
 */
export function lineageAvailable(gem: SupportMetadata, groups: SocketGroupInput[], groupIndex: number, maxCopies = 1): boolean {
  if (!gem.is_lineage) return true;
  const elsewhere = groups.reduce((count, group, index) => index === groupIndex ? count
    : count + group.gems.filter(input => input.skill_id === gem.skill_id).length, 0);
  return elsewhere < maxCopies;
}

export interface SupportPool { gems: SupportMetadata[]; unknown: number; levelBlocked: number; incompatible: number }

/** Track possible truth values while assembling a candidate pool. A type added by
 * another support may be present or absent; actual combinations use exact matching.
 */
function possibleExpression(expression: string[], base: Set<string>, added: Set<string>): Set<boolean> {
  const stack: Set<boolean>[] = [];
  for (const token of expression) {
    if (token === 'NOT') {
      const value = stack.pop();
      if (!value) return new Set([false]);
      stack.push(new Set([...value].map(entry => !entry)));
    } else if (token === 'AND' || token === 'OR') {
      const right = stack.pop();
      const left = stack.pop();
      if (!right || !left) return new Set([false]);
      stack.push(new Set([...left].flatMap(a => [...right].map(b => token === 'AND' ? a && b : a || b))));
    } else stack.push(base.has(token) ? new Set([true]) : added.has(token) ? new Set([false, true]) : new Set([false]));
  }
  if (!stack.length) return new Set([false]);
  return new Set([
    ...(stack.every(values => values.has(false)) ? [false] : []),
    ...(stack.some(values => values.has(true)) ? [true] : []),
  ]);
}

/** Build a permissive type closure; each actual combination is checked again before evaluation. */
export function eligibleSupports(group: SocketGroupInput, catalog: SupportMetadata[], characterLevel: number,
  includeLineage = true): SupportPool {
  const active = group.gems.map(input => catalog.find(gem => gem.skill_id === input.skill_id))
    .filter(gem => known(gem) && !gem.is_support) as SupportMetadata[];
  const all = catalog.filter(gem => gem.is_support && (includeLineage || !gem.is_lineage));
  const unknown = all.filter(gem => !known(gem)).length;
  const withData = all.filter(known);
  const usable = withData.filter(gem => usableSupportLevel(gem, characterLevel) > 0);
  const added = new Set(usable.flatMap(gem => gem.add_skill_types ?? []));
  const possible = usable.filter(gem => active.some(skill => {
    if (skill.cannot_be_supported || (gem.support_gems_only && group.source)) return false;
    const types = new Set(skill.skill_types ?? []);
    return (!(gem.require_skill_types?.length) || possibleExpression(gem.require_skill_types, types, added).has(true))
      && possibleExpression(gem.exclude_skill_types ?? [], types, added).has(false);
  }));
  return { gems: possible, unknown, levelBlocked: withData.length - usable.length,
    incompatible: usable.length - possible.length };
}

export interface SupportPlan {
  supports: GemInput[];
  variant: VariantInput;
  stats: Record<string, number>;
}
export interface SupportOptimization {
  baseline: Record<string, number>;
  plans: SupportPlan[];
  candidates: number;
  evaluated: number;
  unmodeled: number;
}

export function supportVariant(request: CalculateBuildRequest, groupIndex: number,
  supports: GemInput[], catalog: SupportMetadata[]): VariantInput {
  const supportIds = new Set(catalog.filter(gem => gem.is_support).map(gem => gem.skill_id));
  return { socket_groups: (request.socket_groups ?? []).map((group, index) => index === groupIndex
    ? { ...group, gems: [...group.gems.filter(gem => !supportIds.has(gem.skill_id)), ...supports] } : group) };
}

/** Apply only the evaluated gem array, retaining editor metadata for inactive weapon groups. */
export function applySupportPlan(groups: SocketGroupInput[], groupIndex: number, plan: Pick<SupportPlan, 'variant'>): SocketGroupInput[] {
  const gems = plan.variant.socket_groups?.[groupIndex]?.gems;
  return gems ? groups.map((group, index) => index === groupIndex ? { ...group, gems } : group) : groups;
}

const keyOf = (supports: GemInput[]) => supports.map(gem => `${gem.skill_id}:${gem.level}:${gem.quality}:${gem.stat_set_index ?? 1}`).sort().join('|');

/** Every eligible support gets an individual probe. A bounded beam then explores combinations,
 * retaining the current set and neutral candidates so replacements and pair interactions can win.
 * Final results are complete group snapshots, identical to the state applied by the editor.
 */
export async function optimizeSupports(options: {
  request: CalculateBuildRequest;
  groupIndex: number;
  catalog: SupportMetadata[];
  capacity: number;
  objective: Objective;
  includeLineage?: boolean;
  excludedSkillIds?: readonly string[];
  signal?: AbortSignal;
  onProgress?: EvaluateOptions['onProgress'];
  evaluate?: typeof evaluateVariants;
}): Promise<SupportOptimization> {
  const { request, groupIndex, catalog, objective, signal, onProgress } = options;
  signal?.throwIfAborted();
  const group = request.socket_groups?.[groupIndex];
  if (!group?.enabled) throw new Error('Select an enabled skill before optimizing supports.');
  const capacity = Math.max(0, Math.min(5, Math.trunc(options.capacity)));
  const level = request.character?.level ?? 1;
  const excluded = new Set(options.excludedSkillIds);
  const pool = eligibleSupports(group, catalog, level, options.includeLineage ?? true).gems
    .filter(gem => !excluded.has(gem.skill_id) && lineageAvailable(gem, request.socket_groups ?? [], groupIndex));
  const byId = new Map(catalog.map(gem => [gem.skill_id, gem]));
  const equipped = group.gems.filter(gem => byId.get(gem.skill_id)?.is_support);
  const allowedIds = new Set(pool.map(gem => gem.skill_id));
  // Preferences restrict search seeds as well as new candidates. The unchanged
  // request remains the baseline, even when an equipped support is excluded.
  const current = equipped.filter(gem => allowedIds.has(gem.skill_id));
  const toInput = (gem: SupportMetadata): GemInput => equipped.find(input => input.skill_id === gem.skill_id)
    ?? { skill_id: gem.skill_id, level: usableSupportLevel(gem, level), quality: 0 };
  const evaluate = options.evaluate ?? evaluateVariants;
  const identity = await evaluate({ request, variants: [{}], signal });
  if (identity.aborted) throw new DOMException('Support optimization cancelled', 'AbortError');
  signal?.throwIfAborted();
  const identityRow = identity.results[0];
  if (identityRow?.error) throw new Error(identityRow.error);
  const baseline = identity.baseline;
  const baselineUnsupported = new Set(identityRow?.unsupported ?? []);
  const seen = new Set<string>([keyOf(equipped)]);
  const rows: SupportPlan[] = [];
  let evaluated = 0;
  let unmodeled = 0;
  const refinementBudget = 768;
  const estimate = pool.length * 2 + current.length + refinementBudget;
  const run = async (sets: GemInput[][], limit = Infinity): Promise<SupportPlan[]> => {
    const proposed = sets.filter(supports => {
      const key = keyOf(supports);
      if (supports.length > capacity || seen.has(key) || supports.some(gem => !allowedIds.has(gem.skill_id))
        || supports.some(input => !lineageAvailable(byId.get(input.skill_id)!, request.socket_groups ?? [], groupIndex))) return false;
      seen.add(key);
      return true;
    });
    const compatible = await supportSetsCompatible(group, proposed, catalog);
    signal?.throwIfAborted();
    const candidates = proposed.filter((_, index) => compatible[index]).slice(0, limit);
    const accepted: SupportPlan[] = [];
    for (let start = 0; start < candidates.length; start += 512) {
      signal?.throwIfAborted();
      const selected = candidates.slice(start, start + 512);
      const variants = selected.map(supports => supportVariant(request, groupIndex, supports, catalog));
      const result = await evaluate({ request, variants, signal,
        onProgress: done => onProgress?.(evaluated + done, Math.max(estimate, evaluated + candidates.length)) });
      if (result.aborted) throw new DOMException('Support optimization cancelled', 'AbortError');
      signal?.throwIfAborted();
      evaluated += selected.length;
      for (const row of result.results) {
        if (row.error || Object.values(row.stats).some(value => !Number.isFinite(value))) continue;
        if (row.unsupported?.some(line => !baselineUnsupported.has(line))) { unmodeled++; continue; }
        const plan = { supports: selected[row.index], variant: variants[row.index], stats: row.stats };
        accepted.push(plan);
        rows.push(plan);
      }
    }
    return accepted;
  };
  await run([current]);
  const removals = await run(current.map((_, index) => current.filter((_, i) => i !== index)));
  removals.sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective));
  const weakestRemoved = removals[0]?.supports ?? current.slice(0, Math.max(0, capacity - 1));
  const replace = (state: GemInput[], candidate: SupportMetadata, fallback: GemInput[]): GemInput[] => {
    const withoutFamily = state.filter(input => !sameSupportFamily(byId.get(input.skill_id)!, candidate));
    const retained = withoutFamily.length < capacity ? withoutFamily : fallback;
    return [...retained, toInput(candidate)];
  };
  const initialSets = pool.flatMap(gem => [[toInput(gem)], replace(current, gem, weakestRemoved)]);
  const singles = await run(initialSets);
  const compare = (a: SupportPlan, b: SupportPlan) => compareObjectiveStats(a.stats, b.stats, objective);
  singles.sort(compare);
  // Retain a bounded union of strong probes and type-changing supports. Unknown effects
  // never enter the result list; type-changing combinations still receive full recalculation.
  const shortlistIds = new Set(singles.slice(0, 24).flatMap(plan => plan.supports.map(gem => gem.skill_id)));
  for (const gem of pool.filter(gem => gem.add_skill_types?.length).slice(0, 12)) shortlistIds.add(gem.skill_id);
  for (const gem of pool) if (shortlistIds.size < 32) shortlistIds.add(gem.skill_id);
  const shortlist = pool.filter(gem => shortlistIds.has(gem.skill_id));
  let beam: SupportPlan[] = [{ supports: [], variant: supportVariant(request, groupIndex, [], catalog), stats: baseline }];
  let remaining = refinementBudget;
  for (let depth = 1; depth <= capacity && remaining > 0; depth++) {
    const proposals = beam.flatMap(state => shortlist
      .filter(gem => !state.supports.some(input => sameSupportFamily(byId.get(input.skill_id)!, gem)))
      .map(gem => [...state.supports, toInput(gem)]));
    const before = evaluated;
    const round = await run(proposals, remaining);
    remaining -= evaluated - before;
    const depthRows = rows.filter(plan => plan.supports.length === depth).sort(compare);
    beam = depthRows.slice(0, 6);
    if (!beam.length && !round.length) break;
  }
  signal?.throwIfAborted();
  const unique = new Map<string, SupportPlan>();
  for (const plan of rows.sort(compare)) {
    if (!feasibleOf(plan.stats, objective) || compareObjectiveStats(plan.stats, baseline, objective) >= 0) continue;
    if (scoreOf(plan.stats, objective) === 0) continue;
    unique.set(keyOf(plan.supports), plan);
  }
  onProgress?.(evaluated, evaluated);
  return { baseline, plans: [...unique.values()].slice(0, 8), candidates: pool.length, evaluated, unmodeled };
}
