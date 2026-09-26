import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, feasibleOf, scoreOf, type EvaluateOptions, type Objective } from './optimize';
import type { TradePriceCap, TradeRealm, WeightedStat } from './trade';
import { tradeItemVariant, type TradeCatalog, type TradeGem } from './tradeOptimizer';
import { eligibleSupports, lineageAvailable, sameSupportFamily, supportSetsCompatible, type SupportMetadata } from './supportOptimizer';

export interface MarketItem {
  name?: string; baseType?: string; typeLine?: string;
  properties?: { name?: string; type?: number; values: [string, number][] }[];
  requirements?: { name: string; values: [string, number][] }[];
}
export interface Listing { id: string; url?: string; price: { amount: number; currency: string }; item: MarketItem }
export interface MarketResponse { search_mode?: 'weighted' | 'price' | 'budget'; url: string; total: number; sampled: number; listings: Listing[] }
export interface MarketQuery {
  realm: TradeRealm; league: string; category: string; weighted?: WeightedStat[];
  price?: TradePriceCap; maxLevel?: number; gem?: { name: string; level: number; quality: number };
}
export async function searchMarket(query: MarketQuery, signal?: AbortSignal): Promise<MarketResponse> {
  const response = await fetch('/api/trade/search', { method: 'POST', signal,
    headers: { 'content-type': 'application/json' }, body: JSON.stringify(query) });
  const data = await response.json();
  if (!response.ok) throw Object.assign(new Error(data.error ?? `Trade HTTP ${response.status}`), { status: response.status });
  return data as MarketResponse;
}
export interface MarketUpgrade {
  listing: Listing; url: string; text: string; variant: VariantInput;
  stats: Record<string, number>; gain: number; warnings: string[];
}
export interface MarketRanking { baseline: Record<string, number>; upgrades: MarketUpgrade[]; rejected: number }

/** Prices stay in their quoted currency; never compare unlike currencies numerically. */
export function rankMarket(upgrades: MarketUpgrade[], valueCurrency?: string): MarketUpgrade[] {
  const score = (entry: MarketUpgrade) => valueCurrency
    ? entry.listing.price.currency === valueCurrency && entry.listing.price.amount > 0 ? entry.gain / entry.listing.price.amount : -Infinity
    : entry.gain;
  return [...upgrades].sort((a, b) => score(b) - score(a) || b.gain - a.gain ||
    (a.listing.price.currency === b.listing.price.currency ? a.listing.price.amount - b.listing.price.amount : 0));
}

export async function evaluateMarket(options: {
  request: CalculateBuildRequest; slot: string; market: MarketResponse; objective: Objective;
  signal?: AbortSignal; onProgress?: EvaluateOptions['onProgress'];
  evaluate?: typeof evaluateVariants;
  importItems?: (items: unknown[]) => Promise<{ text?: string; warnings?: string[]; error?: string }[]>;
}): Promise<MarketRanking> {
  const { request, slot, market, objective, signal, onProgress } = options;
  signal?.throwIfAborted();
  if (!market.listings.length) return { baseline: {}, upgrades: [], rejected: 0 };
  const importer = options.importItems ?? (await getBackend()).importTradeItems;
  const imported = await importer(market.listings.map(entry => entry.item));
  const candidates = imported.flatMap((entry, index) => entry.text && !entry.error ? [{
    listing: market.listings[index], text: entry.text, warnings: entry.warnings ?? [],
    variant: tradeItemVariant(request, slot, entry.text),
  }] : []);
  const result = await (options.evaluate ?? evaluateVariants)({ request,
    variants: candidates.map(entry => entry.variant), signal, onProgress });
  if (result.aborted) throw new DOMException('Search cancelled', 'AbortError');
  signal?.throwIfAborted();
  const baseScore = scoreOf(result.baseline, objective);
  let rejected = imported.length - candidates.length;
  const upgrades = result.results.flatMap(row => {
    if (row.error) { rejected++; return []; }
    const candidate = candidates[row.index];
    const gain = scoreOf(row.stats, objective) - baseScore;
    if (!Number.isFinite(gain) || gain <= 0) return [];
    return [{ ...candidate, url: candidate.listing.url ?? market.url, stats: row.stats, gain,
      warnings: [...candidate.warnings, ...(row.unsupported ?? [])] }];
  });
  return { baseline: result.baseline, upgrades: rankMarket(upgrades), rejected };
}

export type GemAcquisition = 'skill-adjustment' | 'market-lineage' | 'market-skill';
export function gemAcquisition(gem: SupportMetadata): GemAcquisition {
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
  const gems: SupportMetadata[] = catalog.gems ?? [];
  const byId = new Map(gems.map(gem => [gem.skill_id, gem]));
  const plans: GemPlan[] = [];
  const supports = current.gems.flatMap((gem, index) => byId.get(gem.skill_id)?.is_support ? [index] : []);
  // Imported groups expose occupied sockets, not the unlocked capacity. Only the two
  // initial support sockets are assumed; additional sockets are configured in Skills.
  const capacity = Math.max(2, Math.min(5, supports.length));
  const characterLevel = request.character?.level ?? 1;
  const add = (gem: SupportMetadata, position: number, level: number, quality: number) => {
    const variant = gemVariant(request, group, position, gem, level, quality);
    const replacement = variant.socket_groups![group];
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
    .filter(input => byId.get(input.skill_id)?.is_support)), gems);
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

export async function evaluateGemMarket(request: CalculateBuildRequest, plan: GemPlan, market: MarketResponse,
  objective: Objective, signal?: AbortSignal): Promise<MarketRanking> {
  const property = (item: MarketItem, type: number) => {
    const raw = item.properties?.find(entry => entry.type === type)?.values[0]?.[0];
    return raw === undefined ? undefined : Number.parseInt(raw.replace(/[^\d]/g, ''), 10);
  };
  const candidates = market.listings.flatMap(listing => {
    const level = property(listing.item, 5);
    const quality = property(listing.item, 6) ?? 0;
    if (!level || level > 40 || quality > 100) return [];
    return [{ listing, variant: gemVariant(request, plan.group, plan.position, plan.gem, level, quality),
      text: `${plan.gem.name}\nLevel: ${level}\nQuality: ${quality}` }];
  });
  const result = await evaluateVariants({ request, variants: candidates.map(entry => entry.variant), signal });
  if (result.aborted) throw new DOMException('Search cancelled', 'AbortError');
  signal?.throwIfAborted();
  return { baseline: result.baseline, rejected: market.listings.length - candidates.length,
    upgrades: rankMarket(result.results.flatMap(row => {
      const gain = scoreOf(row.stats, objective) - scoreOf(result.baseline, objective);
      if (row.error || !Number.isFinite(gain) || gain <= 0) return [];
      return [{ ...candidates[row.index], url: candidates[row.index].listing.url ?? market.url, stats: row.stats, gain, warnings: row.unsupported ?? [] }];
    })) };
}
