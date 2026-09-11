import type { CalculateBuildRequest, VariantInput } from '../api/types';
import { evaluateVariants, scoreOf, type EvaluateOptions, type EvaluateResult, type Objective } from './optimize';
import { normalizeTradeLine, type WeightedStat } from './trade';

export interface TradeBase {
  name: string;
  category: string;
  tags: string[];
  level: number;
  implicits: string[];
  domain?: string;
  affix_limit?: number;
}
export interface TradeAffix {
  id: string;
  group: string;
  kind: 'prefix' | 'suffix';
  level: number;
  lines: string[];
  weights: [string, number][];
  stats: { id: string; line: string; value: number }[];
  domain?: string;
}
export interface TradeGem { skill_id: string; name: string; family: string; is_support: boolean; max_level: number }
export interface TradeCatalog { bases: TradeBase[]; mods: TradeAffix[]; gems?: TradeGem[] }

export async function loadTradeCatalog(): Promise<TradeCatalog> {
  const manifest = await (await fetch('/data/manifest.json')).json() as { version: string };
  const response = await fetch(`/data/${manifest.version}/overlay/trade_catalog.json`);
  if (!response.ok) throw new Error('Trade affix catalog unavailable');
  const data = await response.json() as TradeCatalog;
  if (!Array.isArray(data.bases) || !Array.isArray(data.mods)) throw new Error('Invalid trade catalog');
  return data;
}

const SLOT_CATEGORIES: Record<string, string[]> = {
  helmet: ['armour.helmet'], bodyarmour: ['armour.chest'], gloves: ['armour.gloves'],
  boots: ['armour.boots'], amulet: ['accessory.amulet'], ring1: ['accessory.ring'],
  ring2: ['accessory.ring'], belt: ['accessory.belt'],
};

export function basesForSlot(catalog: TradeCatalog, slot: string): TradeBase[] {
  return catalog.bases.filter(base => {
    if (slot.startsWith('Jewel@')) return base.category === 'jewel';
    if (slot === 'Flask 1') return base.category === 'flask.life';
    if (slot === 'Flask 2') return base.category === 'flask.mana';
    if (slot.startsWith('Charm ')) return base.domain === 'charm';
    if (slot === 'weapon1') return base.category.startsWith('weapon.');
    if (slot === 'weapon2') return ['armour.quiver', 'armour.shield', 'armour.buckler', 'armour.focus',
      'weapon.onemace', 'weapon.wand', 'weapon.sceptre', 'weapon.flail', 'weapon.spear'].includes(base.category);
    return SLOT_CATEGORIES[slot]?.includes(base.category) ?? false;
  });
}

/** First matching spawn weight wins, including a zero-weight exclusion (PoB2 semantics). */
export function affixPool(catalog: TradeCatalog, base: TradeBase, itemLevel: number): TradeAffix[] {
  const strongest = new Map<string, TradeAffix>();
  for (const mod of catalog.mods) {
    if (mod.domain !== base.domain) continue;
    if (mod.level > itemLevel || (mod.weights.find(([tag]) => base.tags.includes(tag))?.[1] ?? 0) <= 0) continue;
    // Retain distinct effects in a shared mod group, but only the highest available tier of each effect.
    const key = `${mod.group}:${mod.lines.map(normalizeTradeLine).join('\n')}`;
    const prev = strongest.get(key);
    if (!prev || mod.level > prev.level || (mod.level === prev.level && mod.id > prev.id)) strongest.set(key, mod);
  }
  return [...strongest.values()].sort((a, b) => a.id.localeCompare(b.id));
}

export function combinationLegal(mods: TradeAffix[], limit = 3): boolean {
  return new Set(mods.map(mod => mod.group)).size === mods.length &&
    mods.filter(mod => mod.kind === 'prefix').length <= limit &&
    mods.filter(mod => mod.kind === 'suffix').length <= limit;
}

export function combinationText(base: TradeBase, mods: TradeAffix[], itemLevel: number): string {
  const header = base.affix_limit === 1 ? ['Rarity: MAGIC', base.name] : ['Rarity: RARE', 'Upgrade Reference', base.name];
  return [...header, `Item Level: ${itemLevel}`,
    `Implicits: ${base.implicits.length}`, ...base.implicits, ...mods.flatMap(mod => mod.lines)].join('\n');
}

/** Replace only the selected slot while retaining every other equipped source. */
export function tradeItemVariant(request: CalculateBuildRequest, slot: string, text: string): VariantInput {
  if (slot.startsWith('Jewel@')) {
    const socket_node = Number(slot.slice(6));
    return { jewels: [...(request.jewels ?? []).filter(jewel => jewel.socket_node !== socket_node), { socket_node, text }] };
  }
  if (/^(Flask|Charm) /.test(slot)) {
    return { flasks: [...(request.flasks ?? []).filter(item => item.slot !== slot), { slot, text }] };
  }
  return { set_items: [{ slot, text }] };
}

export interface TradeCombination {
  mods: TradeAffix[];
  text: string;
  stats: Record<string, number>;
  score: number;
}
export interface TradeOptimization {
  baseline: Record<string, number>;
  weighted: WeightedStat[];
  combinations: TradeCombination[];
  evaluated: number;
  limited: boolean;
  minimumWeight: number;
}

interface SearchOptions {
  request: CalculateBuildRequest;
  slot: string;
  base: TradeBase;
  pool: TradeAffix[];
  itemLevel: number;
  objective: Objective;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number) => void;
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
  maxEvaluations?: number;
  beamWidth?: number;
  combinations?: boolean;
}

/** Whole-item evaluation preserves nonlinear interactions; beam pruning bounds browser work.
 * Every legal pair is explored before pruning when the evaluation budget permits.
 * Results are the best evaluated combinations, not a proof of global/market optimality.
 */
export async function optimizeTradeAffixes(options: SearchOptions): Promise<TradeOptimization> {
  const { request, slot, base, pool, itemLevel, objective, signal, onProgress } = options;
  const evaluate = options.evaluate ?? evaluateVariants;
  const cap = options.maxEvaluations ?? 2048;
  const width = options.beamWidth ?? 12;
  let evaluated = 0;
  let limited = false;
  let baseline: Record<string, number> = {};
  const variant = (text: string): VariantInput => tradeItemVariant(request, slot, text);
  const run = async (variants: VariantInput[]) => {
    const results: EvaluateResult['results'] = [];
    for (let start = 0; start < variants.length; start += 512) {
      signal?.throwIfAborted();
      const response = await evaluate({ request, variants: variants.slice(start, start + 512), signal,
        onProgress: (done) => onProgress?.(evaluated + done, cap) });
      if (response.aborted) throw new DOMException('Search cancelled', 'AbortError');
      if (evaluated === 0) baseline = response.baseline;
      results.push(...response.results.map(result => ({ ...result, index: result.index + start })));
      evaluated += Math.min(512, variants.length - start);
    }
    return results;
  };
  const blank = combinationText(base, [], itemLevel);
  // Weight each mapped stat independently, even if it belongs to a hybrid affix.
  const probes = [...new Map(pool.flatMap(mod => mod.stats).map(stat => [stat.id, stat])).values()];
  if (probes.length + 1 >= cap) throw new Error('Affix pool exceeds the search budget');
  const probeResults = await run([variant(blank), ...probes.map(stat => variant(`${blank}\n${stat.line}`))]);
  signal?.throwIfAborted();
  const empty = probeResults.find(result => result.index === 0);
  if (!empty || empty.error) throw new Error(empty?.error ?? 'Unable to evaluate item base');
  const emptyScore = scoreOf(empty.stats, objective);
  const scale = 1000 / Math.max(Math.abs(scoreOf(baseline, objective)), Math.abs(emptyScore), 1);
  const weighted = probeResults.flatMap(result => {
    const stat = probes[result.index - 1];
    if (!stat || result.error) return [];
    const gain = scoreOf(result.stats, objective) - emptyScore;
    if (!Number.isFinite(gain) || gain <= 0 || !stat.value) return [];
    return [{ ...stat, weight: gain / stat.value * scale }];
  }).sort((a, b) => b.weight * b.value - a.weight * a.value).slice(0, 32);
  const minimumWeight = Math.max(0, (scoreOf(baseline, objective) - emptyScore) * scale * 0.5);
  if (options.combinations === false) return { baseline, weighted, evaluated, limited: false,
    minimumWeight, combinations: [] };

  let beam: TradeCombination[] = [{ mods: [], text: blank, stats: empty.stats, score: emptyScore }];
  const all: TradeCombination[] = [];
  const seen = new Set<string>();
  let extensionPool = pool;
  const affixLimit = base.affix_limit ?? 3;
  const maxDepth = affixLimit * 2;
  for (let depth = 1; depth <= maxDepth; depth += 1) {
    const candidates: TradeAffix[][] = [];
    for (const parent of beam) {
      for (const mod of extensionPool) {
        const mods = [...parent.mods, mod].sort((a, b) => a.id.localeCompare(b.id));
        if (!combinationLegal(mods, affixLimit)) continue;
        const key = mods.map(entry => entry.id).join('|');
        if (seen.has(key)) continue;
        seen.add(key);
        candidates.push(mods);
      }
    }
    if (candidates.length === 0) break;
    const remaining = cap - evaluated;
    // Reserve evaluations for deeper combinations even on large affix pools.
    const allowance = Math.max(1, Math.floor(remaining / (maxDepth + 1 - depth)));
    if (candidates.length > allowance) limited = true;
    const selected = candidates.slice(0, Math.min(remaining, allowance));
    if (selected.length === 0) break;
    const texts = selected.map(mods => combinationText(base, mods, itemLevel));
    const results = await run(texts.map(variant));
    const ranked = results.flatMap(result => result.error ? [] : [{
      mods: selected[result.index], text: texts[result.index], stats: result.stats,
      score: scoreOf(result.stats, objective),
    }]).sort((a, b) => b.score - a.score || a.text.localeCompare(b.text));
    all.push(...ranked);
    // Retain every single affix so zero-gain singles can still form useful pairs.
    if (depth === 1) {
      beam = ranked;
      extensionPool = [...ranked.map(entry => entry.mods[0]),
        ...pool.filter(mod => !ranked.some(entry => entry.mods[0].id === mod.id))];
    }
    else {
      if (depth < maxDepth && ranked.length > width) limited = true;
      beam = ranked.slice(0, width);
    }
    if (evaluated >= cap) break;
  }
  signal?.throwIfAborted();
  onProgress?.(evaluated, evaluated);
  return { baseline, weighted, evaluated, limited,
    minimumWeight,
    combinations: all.sort((a, b) => b.score - a.score || a.text.localeCompare(b.text)).slice(0, 5) };
}
