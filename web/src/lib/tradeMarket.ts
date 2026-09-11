import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, feasibleOf, scoreOf, type EvaluateOptions, type Objective } from './optimize';
import type { TradePriceCap, TradeRealm, WeightedStat } from './trade';
import { tradeItemVariant, type TradeCatalog, type TradeGem } from './tradeOptimizer';

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

export interface GemPlan { gem: TradeGem; group: number; position: number; level: number; quality: number; variant: VariantInput; gainPercent?: number; gain?: number; stats?: Record<string, number>; baseline?: Record<string, number> }
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
    gems[position] = { skill_id: gem.skill_id, level, quality };
    return { ...entry, gems };
  }) };
}

/** Score every support family on the existing skill, then explore the best replacement positions.
 * A single purchase is evaluated against all currently equipped gems, preserving interactions.
 */
export async function planGemUpgrades(request: CalculateBuildRequest, catalog: TradeCatalog, group: number,
  objective: Objective, signal?: AbortSignal, onProgress?: EvaluateOptions['onProgress']): Promise<GemPlan[]> {
  const current = request.socket_groups?.[group];
  if (!current?.enabled || current.source || !current.gems.length) return [];
  const byId = new Map(catalog.gems?.map(gem => [gem.skill_id, gem]));
  const plans: GemPlan[] = [];
  const add = (gem: TradeGem, position: number, level: number, quality: number) => {
    plans.push({ gem, group, position, level, quality, variant: gemVariant(request, group, position, gem, level, quality) });
  };
  // First score removing each support to select a useful probe position when all sockets are occupied.
  const supports = current.gems.flatMap((gem, index) => byId.get(gem.skill_id)?.is_support ? [index] : []);
  let probePosition = current.gems.length;
  if (supports.length >= 5) {
    const removal = await evaluateVariants({ request, signal, variants: supports.map(position => ({
      socket_groups: request.socket_groups!.map((entry, index) => index === group
        ? { ...entry, gems: entry.gems.filter((_, i) => i !== position) } : entry),
    })) });
    if (removal.aborted) throw new DOMException('Search cancelled', 'AbortError');
    const best = [...removal.results].filter(row => !row.error).sort((a, b) => scoreOf(b.stats, objective) - scoreOf(a.stats, objective))[0];
    if (!best) return [];
    probePosition = supports[best.index];
  }
  const present = new Set(current.gems.map(gem => gem.skill_id));
  const characterLevel = request.character?.level ?? 1;
  for (const gem of catalog.gems ?? []) {
    if (!gem.is_support || present.has(gem.skill_id)) continue;
    const duplicateFamily = current.gems.some((entry, index) => index !== probePosition && byId.get(entry.skill_id)?.family === gem.family);
    const level = Math.min(gem.max_level, usableGemLevel(gem, characterLevel));
    if (!duplicateFamily && level > 0) add(gem, probePosition, level, 0);
  }
  current.gems.forEach((entry, position) => {
    const gem = byId.get(entry.skill_id);
    if (!gem) return;
    // Include a quality purchase and a level purchase; supports use their actual natural maximum.
    if (entry.quality < 20) add(gem, position, entry.level, 20);
    const level = Math.min(gem.max_level + 1, usableGemLevel(gem, characterLevel));
    if (!gem.is_support && entry.level < level) add(gem, position, level, Math.max(entry.quality, 20));
  });
  const ranked: { plan: GemPlan; gain: number }[] = [];
  const evaluate = async (batch: GemPlan[]) => {
    for (let start = 0; start < batch.length; start += 512) {
      signal?.throwIfAborted();
      const selected = batch.slice(start, start + 512);
      const result = await evaluateVariants({ request, variants: selected.map(plan => plan.variant), signal,
        onProgress: done => onProgress?.(start + done, batch.length) });
      if (result.aborted) throw new DOMException('Search cancelled', 'AbortError');
      for (const row of result.results) {
        const gain = scoreOf(row.stats, objective) - scoreOf(result.baseline, objective);
        if (!row.error && feasibleOf(row.stats, objective) && compareObjectiveStats(row.stats, result.baseline, objective) < 0) ranked.push({ plan: { ...selected[row.index], gain, stats: row.stats, baseline: result.baseline,
          gainPercent: gain / Math.max(Math.abs(scoreOf(result.baseline, objective)), 1) * 100 }, gain });
      }
    }
  };
  await evaluate(plans);
  ranked.sort((a, b) => compareObjectiveStats(a.plan.stats!, b.plan.stats!, objective));
  const refinements: GemPlan[] = [];
  for (const { plan } of ranked.slice(0, 8)) {
    if (!plan.gem.is_support || present.has(plan.gem.skill_id)) continue;
    for (const position of supports) {
      if (position === plan.position || current.gems.some((entry, index) => index !== position && byId.get(entry.skill_id)?.family === plan.gem.family)) continue;
      refinements.push({ ...plan, position, variant: gemVariant(request, group, position, plan.gem, plan.level, plan.quality) });
    }
  }
  await evaluate(refinements);
  signal?.throwIfAborted();
  const unique = new Map<string, GemPlan>();
  for (const { plan } of ranked.sort((a, b) => compareObjectiveStats(a.plan.stats!, b.plan.stats!, objective))) {
    const key = `${plan.gem.skill_id}:${plan.level}:${plan.quality}`;
    if (!unique.has(key)) unique.set(key, plan);
  }
  return [...unique.values()].slice(0, 3);
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
