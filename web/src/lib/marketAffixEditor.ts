import type { ItemLineJson } from '../api/types';
import { itemForAffixProbes } from './equipmentScore';
import { replacementAffixes } from './itemReplacement';
import { normalizeTradeLine } from './trade';
import { combinationLegal, type TradeAffix, type TradeBase, type TradeCatalog } from './tradeOptimizer';

export interface EditableAffix {
  key: string;
  modId?: string;
  indices: number[];
  original: string[];
  /** A common position in each independent numeric range, not a spawn probability. */
  roll: number;
  edited: boolean;
  fractured?: boolean;
}
export interface AffixDraft {
  text: string;
  base: TradeBase;
  rows: EditableAffix[];
  sourceIndices: number[];
  itemLevel: number | null;
  limit: number;
  locked?: 'rarity' | 'corrupted' | 'classification';
}
export type AffixAliases = ReadonlyMap<string, string>;
const clean = (line: string) => line.replace(/\{[^}]*\}/g, '').replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
  .replace(/\[([^\]]*)\]/g, '$1').replace(/\s*\((?:implicit|enchant|crafted|fractured|rune|tier:\s*\d+)\)/gi, '').trim();
const signature = (line: string) => normalizeTradeLine(clean(line)).toLocaleLowerCase().replace(/\s+/g, ' ');
const numbers = (line: string) => [...clean(line).matchAll(/-?\d+(?:\.\d+)?/g)].map(match => Number(match[0]));

/** Keep exact midpoints (including fractions): this is an estimate, not an actual integer roll. */
export function affixRollLines(mod: TradeAffix, roll = 0.5): string[] {
  if (!mod.roll_lines) throw new Error('missing-ranges');
  if (!Number.isFinite(roll) || roll < 0 || roll > 1) throw new Error('invalid-roll');
  return mod.roll_lines.map(line => line.replace(/\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)/g,
    (_, low: string, high: string) => String(Number((Number(low) + (Number(high) - Number(low)) * roll).toFixed(4)))));
}

/** Full tier pool: the trade-search pool intentionally retains only its strongest tiers. */
export function editableAffixPool(catalog: TradeCatalog, base: TradeBase): TradeAffix[] {
  return catalog.mods.filter(mod => mod.domain === base.domain && mod.roll_lines?.length === mod.lines.length
    && (mod.weights.find(([tag]) => base.tags.includes(tag))?.[1] ?? 0) > 0);
}

export function affixTier(mod: TradeAffix, pool: readonly TradeAffix[]): number {
  const magnitude = (candidate: TradeAffix) => affixRollLines(candidate, 1).flatMap(numbers).reduce((sum, n) => sum + Math.abs(n), 0);
  const family = pool.filter(candidate => candidate.group === mod.group && candidate.kind === mod.kind
    && candidate.lines.map(signature).sort().join('\n') === mod.lines.map(signature).sort().join('\n'))
    .sort((a, b) => b.level - a.level || magnitude(b) - magnitude(a) || b.id.localeCompare(a.id));
  return family.findIndex(candidate => candidate.id === mod.id) + 1;
}

function matchesLine(value: string, mod: TradeAffix, lineIndex: number, aliases: AffixAliases): boolean {
  const expected = mod.lines[lineIndex];
  const actual = signature(value);
  if (![expected, aliases.get(expected)].some(line => line && signature(line) === actual)) return false;
  const low = numbers(affixRollLines(mod, 0)[lineIndex]);
  const high = numbers(affixRollLines(mod, 1)[lineIndex]);
  const values = numbers(value);
  return values.length === low.length && values.every((number, index) => number >= Math.min(low[index], high[index])
    && number <= Math.max(low[index], high[index]));
}

/** Clipboard lines may sum independent affixes. Check only overlapping local
 * signatures and interval bounds; this is a conservative ambiguity test, not
 * an exhaustive reconstruction or a claim that every possible sum is legal.
 */
function mergedAffixIndices(explicit: { index: number; text: string }[], pool: TradeAffix[], limit: number, aliases: AffixAliases): Set<number> {
  type RollLine = { low: number[]; high: number[]; targets: { index: number; values: number[] }[] };
  const source = explicit.map(row => ({ ...row, key: signature(row.text), values: numbers(row.text) }));
  const within = (value: number, low: number, high: number) => value >= Math.min(low, high) - 1e-8 && value <= Math.max(low, high) + 1e-8;
  const descriptors = pool.flatMap(mod => {
    const minimum = affixRollLines(mod, 0), maximum = affixRollLines(mod, 1);
    const lines: RollLine[] = mod.lines.map((line, index) => {
      const low = numbers(minimum[index]), high = numbers(maximum[index]);
      const keys = new Set([signature(line), ...(aliases.has(line) ? [signature(aliases.get(line)!)] : [])]);
      const targets = source.filter(row => keys.has(row.key) && row.values.length > 0 && row.values.length === low.length
        && row.values.every((value, i) => Math.min(low[i], high[i]) >= 0 ? value >= Math.min(low[i], high[i])
          : Math.max(low[i], high[i]) <= 0 ? value <= Math.max(low[i], high[i]) : true));
      return { low, high, targets };
    });
    return lines.every(line => line.targets.length > 0) ? [{ mod, lines }] : [];
  });
  const byIndex = new Map<number, { mod: TradeAffix; line: RollLine }[]>();
  for (const descriptor of descriptors) for (const line of descriptor.lines) for (const target of line.targets) {
    const entries = byIndex.get(target.index) ?? [];
    entries.push({ mod: descriptor.mod, line }); byIndex.set(target.index, entries);
  }
  const maySum = (mod: TradeAffix, line: RollLine, target: RollLine['targets'][number]) => {
    const others = (byIndex.get(target.index) ?? []).filter(other => combinationLegal([mod, other.mod], limit));
    if (!others.length) return false;
    return target.values.every((value, index) => {
      let lower = 0, upper = 0;
      const minima: number[] = [], maxima: number[] = [];
      for (const kind of ['prefix', 'suffix'] as const) {
        const capacity = limit - Number(mod.kind === kind);
        if (capacity <= 0) continue;
        const groups = new Map<string, { low: number; high: number }>();
        for (const other of others.filter(entry => entry.mod.kind === kind)) {
          const existing = groups.get(other.mod.group);
          groups.set(other.mod.group, { low: Math.min(existing?.low ?? Infinity, other.line.low[index], other.line.high[index]),
            high: Math.max(existing?.high ?? -Infinity, other.line.low[index], other.line.high[index]) });
        }
        const lows = [...groups.values()].map(group => group.low).sort((a, b) => a - b);
        const highs = [...groups.values()].map(group => group.high).sort((a, b) => b - a);
        minima.push(...lows); maxima.push(...highs);
        lower += lows.filter(number => number < 0).slice(0, capacity).reduce((sum, number) => sum + number, 0);
        upper += highs.filter(number => number > 0).slice(0, capacity).reduce((sum, number) => sum + number, 0);
      }
      // At least one other group must contribute. Per-coordinate envelopes may
      // include impossible combinations; rejecting those as unresolved is safe.
      if (lower === 0) lower = Math.min(...minima);
      if (upper === 0) upper = Math.max(...maxima);
      return within(value, Math.min(line.low[index], line.high[index]) + lower, Math.max(line.low[index], line.high[index]) + upper)
        // Numbers embedded in a condition may be fixed rather than additive stats.
        || (line.low[index] === line.high[index] && value === line.low[index]
          && others.some(other => value === other.line.low[index] && value === other.line.high[index]));
    });
  };
  const ambiguous = new Set<number>();
  for (const descriptor of descriptors) {
    let shared = false;
    const possible: number[] = [];
    const everyLineFits = descriptor.lines.every(line => {
      let fits = false;
      for (const target of line.targets) {
        const sum = maySum(descriptor.mod, line, target);
        if (sum || target.values.every((value, index) => within(value, line.low[index], line.high[index]))) {
          possible.push(target.index); fits = true; shared ||= sum;
        }
      }
      return fits;
    });
    if (everyLineFits && shared) possible.forEach(index => ambiguous.add(index));
  }
  return ambiguous;
}

/** Ambiguous combined/hybrid stats remain unknown. Never infer empty affix capacity from line count. */
export function createAffixDraft(text: string, base: TradeBase, classified: ItemLineJson[], pool: TradeAffix[], aliases: AffixAliases = new Map()): AffixDraft {
  const source = text.split('\n');
  const level = text.match(/^Item Level:\s*(\d+)/mi);
  const rarity = text.match(/^Rarity:\s*(\w+)/mi)?.[1].toUpperCase();
  const draft: AffixDraft = { text, base, rows: [], sourceIndices: [], itemLevel: level ? Number(level[1]) : null,
    limit: rarity === 'MAGIC' ? 1 : base.affix_limit ?? 3 };
  if (!['RARE', 'MAGIC'].includes(rarity ?? '')) draft.locked = 'rarity';
  if (/^(?:(?:Twice )?Corrupted|Sanctified|Mirrored|Unidentified|已腐化|已镜像|已鏡像|未鉴定|未鑑定)$/mi.test(text)) draft.locked = 'corrupted';
  const allowed = new Set(replacementAffixes(classified).filter(line => line.kind === 'explicit'));
  const explicit: { index: number; text: string }[] = [];
  let cursor = 0;
  for (const line of classified) {
    const index = source.findIndex((raw, i) => i >= cursor && clean(raw) === clean(line.text));
    if (index < 0) { if (allowed.has(line)) draft.locked = 'classification'; continue; }
    cursor = index + 1;
    if (allowed.has(line)) explicit.push({ index, text: line.text });
  }
  const eligible = pool.filter(mod => draft.itemLevel === null || mod.level <= draft.itemLevel);
  const ambiguous = mergedAffixIndices(explicit, eligible, draft.limit, aliases);
  const candidates = eligible.flatMap(mod => {
    const selected: number[] = [];
    for (let i = 0; i < mod.lines.length; i++) {
      const matches = explicit.filter(row => !selected.includes(row.index) && matchesLine(row.text, mod, i, aliases));
      if (matches.length !== 1) return [];
      selected.push(matches[0].index);
    }
    return [{ mod, indices: selected.sort((a, b) => a - b) }];
  });
  const used = new Set<number>();
  for (const row of explicit) {
    if (used.has(row.index)) continue;
    const matches = candidates.filter(candidate => candidate.indices.includes(row.index));
    const keys = new Set(matches.map(candidate => `${candidate.mod.kind}:${candidate.mod.group}:${candidate.indices.join(',')}`));
    const chosen = keys.size === 1 ? matches.sort((a, b) => b.mod.level - a.mod.level || b.mod.id.localeCompare(a.mod.id))[0] : undefined;
    const unique = chosen && chosen.indices.every(index => !ambiguous.has(index) && candidates.filter(candidate => candidate.indices.includes(index))
      .every(candidate => `${candidate.mod.kind}:${candidate.mod.group}:${candidate.indices.join(',')}` === [...keys][0]));
    const indices = unique ? chosen.indices : [row.index];
    indices.forEach(index => used.add(index));
    draft.rows.push({ key: `original-${row.index}`, ...(unique ? { modId: chosen.mod.id } : {}),
      indices, original: indices.map(index => source[index]), roll: 0.5, edited: false,
      fractured: indices.some(index => /\{fractured\}|\(fractured\)/i.test(source[index])) });
  }
  draft.sourceIndices = explicit.map(row => row.index);
  return draft;
}

export function availableAffixes(draft: AffixDraft, pool: TradeAffix[], characterLevel: number, replacingKey?: string): TradeAffix[] {
  if (draft.locked || draft.itemLevel === null) return [];
  if (replacingKey && draft.rows.find(row => row.key === replacingKey)?.fractured) return [];
  const others = draft.rows.filter(row => row.key !== replacingKey);
  const byId = new Map(pool.map(mod => [mod.id, mod]));
  const known = others.flatMap(row => row.modId && byId.has(row.modId) ? [byId.get(row.modId)!] : []);
  // Existing unresolved lines may be replaced explicitly, but cannot be treated as free slots.
  if (!replacingKey && others.some(row => !row.modId)) return [];
  return pool.filter(mod => mod.level <= draft.itemLevel! && Math.max(draft.base.level, Math.floor(mod.level * 0.8)) <= characterLevel
    && combinationLegal([...known, mod], draft.limit));
}

export function validateAffixDraft(draft: AffixDraft, pool: TradeAffix[], characterLevel: number): string | undefined {
  if (draft.locked) return draft.locked;
  const source = draft.text.split('\n');
  if (draft.sourceIndices.some(index => /\{fractured\}|\(fractured\)/i.test(source[index])
    && !draft.rows.some(row => row.indices.includes(index) && !row.edited && row.original.includes(source[index])))) return 'fractured';
  if (draft.itemLevel === null || !Number.isInteger(draft.itemLevel) || draft.itemLevel < 1 || draft.itemLevel > 100) return 'item-level';
  const mods = draft.rows.map(row => pool.find(mod => mod.id === row.modId));
  if (mods.some(mod => !mod)) return 'unknown';
  if (!combinationLegal(mods as TradeAffix[], draft.limit)) return 'affix-limit';
  if (mods.some(mod => mod!.level > draft.itemLevel! || Math.floor(mod!.level * 0.8) > characterLevel) || draft.base.level > characterLevel) return 'level';
  if (draft.rows.some(row => !Number.isFinite(row.roll) || row.roll < 0 || row.roll > 1)) return 'invalid-roll';
  return undefined;
}

/** Write complete item text; local property totals must be rebuilt after editing local modifiers. */
export function buildAffixItem(draft: AffixDraft, pool: TradeAffix[], characterLevel: number): string {
  const issue = validateAffixDraft(draft, pool, characterLevel);
  if (issue) throw new Error(issue);
  const mods = draft.rows.map(row => pool.find(mod => mod.id === row.modId)!);
  // Include source rows explicitly removed by the player, not just retained rows.
  const removed = new Set(draft.sourceIndices);
  const generated = draft.rows.flatMap((row, index) => row.edited ? affixRollLines(mods[index], row.roll) : row.original);
  const source = draft.text.split('\n');
  const at = Math.min(...removed, source.length);
  const raw = [...source.slice(0, at).filter((_, index) => !removed.has(index)), ...generated,
    ...source.slice(at).filter((_, index) => !removed.has(index + at))].join('\n');
  const requiredLevel = Math.max(draft.base.level, ...mods.map(mod => Math.floor(mod.level * 0.8)),
    ...[...draft.text.matchAll(/(?:^|,|Requirements:)\s*(?:Requires\s+)?(?:Level|LevelReq):?\s*(\d+)/gmi)].map(match => Number(match[1])));
  const rebuilt = itemForAffixProbes(raw).split('\n').filter(line => !/^(?:Physical Damage|Elemental Damage|Chaos Damage|Critical Hit Chance|Attacks per Second|Prefix|Suffix):/i.test(line.trim()))
    .map(line => /^Item Level:/i.test(line) ? `Item Level: ${draft.itemLevel}` : line);
  if (!rebuilt.some(line => /^Item Level:/i.test(line))) rebuilt.push(`Item Level: ${draft.itemLevel}`);
  rebuilt.push(`LevelReq: ${requiredLevel}`);
  return rebuilt.join('\n');
}
