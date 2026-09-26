import { getBackend } from '../api/backend';
import { situationalAffix, type SituationalAffix } from './tradeMechanics';
import type { CalculateBuildRequest, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, feasibleOf, scoreOf, type EvaluateOptions, type EvaluateResult, type Objective } from './optimize';
import { normalizeTradeLine, tradeQueryWeights, type TradeStatTemplate, type WeightedStat } from './trade';
import { itemForAffixProbes, scoreEquipment, tradeStatRemovals, type EquipmentScore } from './equipmentScore';

export interface TradeBase {
  name: string;
  category: string;
  tags: string[];
  level: number;
  implicits: string[];
  domain?: string;
  affix_limit?: number;
  radius?: string;
}
export interface TradeAffix {
  id: string;
  group: string;
  kind: 'prefix' | 'suffix';
  level: number;
  lines: string[];
  /** Original PoB2 ranges, used only for manual item simulation. */
  roll_lines?: string[];
  weights: [string, number][];
  stats: TradeStatTemplate[];
  domain?: string;
}
export interface TradeGem {
  skill_id: string; name: string; family: string; is_support: boolean; max_level: number; level_requirements?: number[];
  is_lineage?: boolean;
  skill_types?: string[];
  require_skill_types?: string[];
  exclude_skill_types?: string[];
  add_skill_types?: string[];
  support_gems_only?: boolean;
  cannot_be_supported?: boolean;
  compatibility_known?: boolean;
  families?: string[];
}
export interface TradeSearchMod {
  id: string;
  source: 'alloy' | 'essence' | 'desecrated' | 'breach' | 'influence' | 'corrupted';
  categories: string[];
  jewel_types?: JewelSearchType[];
  level: number;
  lines: string[];
  stats: TradeStatTemplate[];
}
export interface TradeCatalog { bases: TradeBase[]; mods: TradeAffix[]; search_mods?: TradeSearchMod[]; gems?: TradeGem[] }

export async function loadTradeCatalog(): Promise<TradeCatalog> {
  const data = await (await getBackend()).loadTradeCatalog() as TradeCatalog;
  if (!data || !Array.isArray(data.bases) || !Array.isArray(data.mods)) throw new Error('Invalid trade catalog');
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

export type JewelSearchType = 'base' | 'radius';

export function jewelSearchType(base: TradeBase): JewelSearchType {
  return base.tags.includes('radius_jewel') ? 'radius' : 'base';
}

/** The base is an internal probe reference, never a market base restriction. */
export function referenceBase(catalog: TradeCatalog, slot: string, itemText = '', category?: string, maxLevel = 100, jewelType?: JewelSearchType): TradeBase | undefined {
  const available = basesForSlot(catalog, slot).filter(base => !jewelType || jewelSearchType(base) === jewelType)
    .sort((a, b) => slot.startsWith('Jewel@') ? a.tags.length - b.tags.length : 0);
  const lines = new Set(itemText.split('\n').map(line => line.trim()));
  const equipped = available.find(base => lines.has(base.name));
  const searchCategory = category ?? equipped?.category;
  return equipped && equipped.level <= maxLevel && (!category || equipped.category === category) ? equipped
    : available.find(base => base.level <= maxLevel && (!searchCategory || base.category === searchCategory));
}

export function categoryAffixPool(catalog: TradeCatalog, category: string, itemLevel = 100, maxLevel = 100, jewelType?: JewelSearchType): TradeAffix[] {
  return [...new Map(catalog.bases.filter(base => base.category === category && base.level <= maxLevel && (!jewelType || jewelSearchType(base) === jewelType))
    .flatMap(base => affixPool(catalog, base, itemLevel)).map(mod => [mod.id, mod])).values()];
}

/** PoB2 and GGG crafting categories apply independently of natural spawn weights. */
export function categorySearchMods(catalog: TradeCatalog, category: string, itemLevel = 100, jewelType?: JewelSearchType): TradeSearchMod[] {
  return (catalog.search_mods ?? []).filter(mod => mod.categories.includes(category) && mod.level <= itemLevel
    && (!jewelType || !mod.jewel_types || mod.jewel_types.includes(jewelType)));
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
  return [...header, `Item Level: ${itemLevel}`, ...(base.radius ? [`Radius: ${base.radius}`] : []),
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
  situational?: SituationalAffix[];
  baseline: Record<string, number>;
  weighted: (WeightedStat & { gain: number; gainPercent: number })[];
  combinations: TradeCombination[];
  evaluated: number;
  limited: boolean;
  minimumWeight: number;
  unsupported: string[];
  currentItemScore: EquipmentScore | null;
  scoreWarnings: ('partial-current-score' | 'nonlinear' | 'base-dependent' | 'constraints-not-in-query'|'unmodeled-candidates')[];
}

interface SearchOptions {
  request: CalculateBuildRequest;
  slot: string;
  base: TradeBase;
  pool: TradeAffix[];
  searchMods?: TradeSearchMod[];
  itemLevel: number;
  objective: Objective;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number) => void;
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
  /** Combination budget; a complete mandatory stat pass can raise this floor. */
  maxEvaluations?: number;
  beamWidth?: number;
  combinations?: boolean;
  combinationPool?: TradeAffix[];
}

/** Whole-item evaluation preserves nonlinear interactions; beam pruning bounds browser work.
 * Every legal pair is explored before pruning when the evaluation budget permits.
 * Results are the best evaluated combinations, not a proof of global/market optimality.
 */
export async function optimizeTradeAffixes(options: SearchOptions): Promise<TradeOptimization> {
  const { request, slot, base, pool, itemLevel, objective, signal, onProgress } = options;
  const evaluate = options.evaluate ?? evaluateVariants;
  let cap = options.maxEvaluations ?? 2048;
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
  // Weight each mapped stat independently, even if it belongs to a hybrid affix.
  const templates = [...pool, ...(options.searchMods ?? [])].flatMap(mod => mod.stats);
  const probes = [...new Map([...templates].sort((a, b) => Math.abs(a.value) - Math.abs(b.value))
    .map(stat => [stat.id, stat])).values()];
  const currentText = slot.startsWith('Jewel@')
    ? request.jewels?.find(jewel => jewel.socket_node === Number(slot.slice(6)))?.text
    : (/^(Flask|Charm) /.test(slot) ? request.flasks : request.items)?.find(item => item.slot === slot)?.text;
  const sameBase = currentText?.split('\n').some(line => line.trim() === base.name);
  const quality = sameBase ? currentText?.split('\n').find(line => /^Quality:/.test(line.trim())) : undefined;
  const blank = [combinationText(base, [], itemLevel), quality].filter(Boolean).join('\n');
  const current = sameBase && currentText ? itemForAffixProbes(currentText) : undefined;
  const contexts = current ? [blank, current] : [blank];
  // Existing stats are removed to measure their realized marginal contribution.
  // Adding a second full roll instead would undervalue capped resistance/accuracy
  // and can recommend replacing the very affix that keeps the build at its cap.
  const currentRemovals = current ? tradeStatRemovals(current, templates) : new Map();
  const removals = probes.map(stat => currentRemovals.get(stat.id));
  const passiveIds = new Set(probes.filter(stat => stat.kind === 'granted_passive').map(stat => stat.id));
  let passiveReference = current;
  for (const id of currentRemovals.keys()) {
    if (passiveIds.has(id) && passiveReference) passiveReference = tradeStatRemovals(passiveReference, templates).get(id)?.text ?? passiveReference;
  }
  // An essence allocates one outcome. Probe alternatives against the current
  // item with that craft removed, preserving anoints on other equipped items.
  const separatePassiveReference = passiveReference !== current;
  const passiveReferenceIndex = (probes.length + 1) * contexts.length;
  const probeVariants = contexts.flatMap((text, context) => [variant(text),
    ...probes.map((stat, index) => variant(context === 1 && stat.kind === 'granted_passive'
      ? `${passiveReference}\n${stat.line}` : context === 1 && removals[index]
      ? removals[index]!.text : `${text}\n${stat.id.startsWith('enchant.') ? '{enchant}' : ''}${stat.line}`))]);
  if (separatePassiveReference) probeVariants.push(variant(passiveReference!));
  // Every mapped outcome must be considered before pruning combinations. A
  // larger data pack must not fail the whole slot or favor early passive names.
  cap = Math.max(cap, probeVariants.length);
  const probeResults = await run(probeVariants);
  signal?.throwIfAborted();
  const empty = probeResults.find(result => result.index === 0);
  if (!empty || empty.error) throw new Error(empty?.error ?? 'Unable to evaluate item base');
  const emptyScore = scoreOf(empty.stats, objective);
  const scale = 1000 / Math.max(Math.abs(scoreOf(baseline, objective)), Math.abs(emptyScore), 1);
  let nonlinear = false;
  let skippedUnmodeled = false;
  const modeledProbe = (reference: EvaluateResult['results'][number], result: EvaluateResult['results'][number]) => {
    const before = new Set(reference.unsupported ?? []);
    const after = new Set(result.unsupported ?? []);
    // Removing an unsupported effect can also change whether the remaining item is injected.
    if ([...before].some(line => !after.has(line)) || [...after].some(line => !before.has(line))) {
      skippedUnmodeled = true;
      return false;
    }
    return true;
  };
  const weighted = tradeQueryWeights(probes.flatMap((stat, index) => {
    if (situationalAffix(stat, {}, {})?.kind === 'buff') { skippedUnmodeled = true; return []; }
    const unitGains = contexts.flatMap((_, context) => {
      const offset = context * (probes.length + 1);
      const passive = context === 1 && stat.kind === 'granted_passive';
      const reference = probeResults.find(row => row.index === (passive && separatePassiveReference ? passiveReferenceIndex : offset));
      const result = probeResults.find(row => row.index === offset + index + 1);
      if (!reference || reference.error || !result || result.error) return [];
      if (!modeledProbe(reference, result)) return [];
      const removed = context === 1 && !passive ? removals[index] : undefined;
      const value = removed?.value ?? stat.value;
      if (!value) return [];
      const delta = scoreOf(result.stats, objective) - scoreOf(reference.stats, objective);
      return [(removed ? -delta : delta) / value];
    });
    if (unitGains.length === 2 && Math.abs(unitGains[0] - unitGains[1]) > Math.max(...unitGains.map(Math.abs), 1e-9) * 0.25) nonlinear = true;
    const gain = unitGains.reduce((sum, value) => sum + value, 0) / Math.max(unitGains.length, 1) * stat.value;
    if (!Number.isFinite(gain) || gain <= 0 || !stat.value) return [];
    return [{ ...stat, weight: gain / stat.value * scale, gain,
      gainPercent: gain / Math.max(Math.abs(scoreOf(baseline, objective)), 1) * 100 }];
  }).sort((a, b) => b.weight * b.value - a.weight * a.value));
  const situational = probes.flatMap((stat, index) => {
    if (weighted.some(weight => weight.id === stat.id)) return [];
    const conditional = situationalAffix(stat, {}, {});
    if (conditional?.kind === 'buff') return [conditional];
    const result = probeResults.find(row => row.index === index + 1);
    return !result || result.error || !modeledProbe(empty, result) ? [] : situationalAffix(stat, empty.stats, result.stats) ?? [];
  });
  const unsupported = [...new Set(probeResults.filter(row => row.index % (probes.length + 1) === 0)
    .flatMap(row => row.unsupported ?? []))];
  const currentItemScore = currentText ? scoreEquipment(currentText, weighted, templates) : null;
  // A same-query Sum is a comparable search threshold. A whole-build DPS delta
  // is not in these units and the previous arbitrary 50% threshold admitted downgrades.
  const minimumWeight = currentItemScore?.complete ? Math.max(0, Math.ceil((currentItemScore.score - 1e-9) * 1000) / 1000) : 0;
  const scoreWarnings: TradeOptimization['scoreWarnings'] = [
    ...(currentItemScore && !currentItemScore.complete ? ['partial-current-score' as const] : []),
    ...(nonlinear ? ['nonlinear' as const] : []),
    ...(base.category.startsWith('weapon.') || base.category.startsWith('armour.') || base.implicits.length
      ? ['base-dependent' as const] : []),
    ...(objective.constraints.length || objective.softMinimums?.length ? ['constraints-not-in-query' as const] : []),
    ...(skippedUnmodeled ? ['unmodeled-candidates' as const] : []),
  ];
  if (options.combinations === false) return { baseline, weighted, evaluated, limited: false,
    minimumWeight, combinations: [], unsupported, situational, currentItemScore, scoreWarnings };

  const baselineUnsupported = new Set(unsupported);
  let beam: TradeCombination[] = [{ mods: [], text: blank, stats: empty.stats, score: emptyScore }];
  const all: TradeCombination[] = [];
  const seen = new Set<string>();
  // Rank before truncation so an evaluation cap cannot favor alphabetical IDs.
  // Keep zero-gain affixes eligible: useful interactions can require both parts.
  const weightById = new Map(weighted.map(stat => [stat.id, stat.weight]));
  const estimate = (mod: TradeAffix) => mod.stats.reduce((sum, stat) => sum + (weightById.get(stat.id) ?? 0) * stat.value, 0);
  const legalPool = options.combinationPool ?? pool;
  let extensionPool = [...legalPool].sort((a, b) => estimate(b) - estimate(a) || a.id.localeCompare(b.id));
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
    const texts = selected.map(mods => [combinationText(base, mods, itemLevel), quality].filter(Boolean).join('\n'));
    const results = await run(texts.map(variant));
    const ranked = results.flatMap(result => {
      if (result.unsupported?.some(line => !baselineUnsupported.has(line))) { skippedUnmodeled = true; return []; }
      return result.error || Object.values(result.stats).some(value => !Number.isFinite(value)) ? [] : [{
      mods: selected[result.index], text: texts[result.index], stats: result.stats,
      score: scoreOf(result.stats, objective),
    }]; }).sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.text.localeCompare(b.text));
    all.push(...ranked);
    // Retain every single affix so zero-gain singles can still form useful pairs.
    if (depth === 1) {
      beam = ranked;
      extensionPool = [...ranked.map(entry => entry.mods[0]),
        ...extensionPool.filter(mod => !ranked.some(entry => entry.mods[0].id === mod.id))];
    }
    else {
      if (depth < maxDepth && ranked.length > width) limited = true;
      beam = ranked.slice(0, width);
    }
    if (evaluated >= cap) break;
  }
  signal?.throwIfAborted();
  onProgress?.(evaluated, evaluated);
  if (skippedUnmodeled && !scoreWarnings.includes('unmodeled-candidates')) scoreWarnings.push('unmodeled-candidates');
  return { baseline, weighted, evaluated, limited, currentItemScore, scoreWarnings,
    minimumWeight, unsupported, situational,
    combinations: all.filter(entry => feasibleOf(entry.stats, objective))
      .sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.text.localeCompare(b.text)).slice(0, 5) };
}
