import { lineValue, normalizeTradeLine, tradeQueryWeights, type TradeStatTemplate, type WeightedStat } from './trade';

export interface ItemScoreContribution {
  id: string;
  line: string;
  value: number;
  weight: number;
  score: number;
}

export interface EquipmentScore {
  /** The official query's weighted Sum, never a DPS estimate. */
  score: number;
  /** False when a relevant line could not be mapped without guessing. */
  complete: boolean;
  matchedLines: number;
  totalLines: number;
  contributions: ItemScoreContribution[];
  unscoredLines: string[];
}

type StatTemplate = TradeStatTemplate;
interface ItemLine { index: number; line: string; namespace: string; implicitHeader?: number; uncertain?: boolean }

const METADATA = /^(?:Item Class|Rarity|Item Level|Requirements|Level|LevelReq|Str|Dex|Int|Sockets|Rune|Soul Core|Charm Slots|Armour|Evasion(?: Rating)?|Energy Shield|Spirit|(?:Runic )?Ward|Block|Quality|Physical Damage|Elemental Damage|Chaos Damage|Critical Hit Chance|Attacks per Second|Weapon Range|Unique ID|Item ID|Note|Selected Variant|Variant|Radius|Talisman Tier|Limited to|Crafted|Prefix|Suffix|Catalyst|CatalystQuality):|^(?:Corrupted|Mirrored|Unidentified|Has Alt Variant|Requires\b)/;
const sourceMarker = (line: string, source: string) => new RegExp(`\\{${source}\\}|\\(${source}\\)`, 'i').test(line);

function cleanLine(line: string): string {
  return line.replace(/\{[^}]*\}/g, '').replace(/\s*\((?:[a-z]+|tier:\s*\d+)\)/gi, '')
    .replace(/\s*\[(?:augmented|crafted|fractured)\]/gi, '').trim();
}

/** Preserve original indices so an actual affix can be removed for a marginal probe. */
function itemLines(text: string): ItemLine[] {
  const lines = text.split('\n').map(line => line.trim());
  const rarity = lines.findIndex(line => /^Rarity:/i.test(line));
  if (rarity < 0) return [];
  const separator = lines.findIndex((line, index) => index > rarity && line === '--------');
  let start = rarity + 1;
  const maxNames = /(?:RARE|UNIQUE)/i.test(lines[rarity]) ? 2 : 1;
  for (let names = 0; start < lines.length && names < maxNames && !METADATA.test(lines[start]); start += 1) {
    if (lines[start]) names += 1;
  }
  if (maxNames === 1 && lines[start] === lines[rarity + 1]) start += 1;
  // Clipboard magic items may have a separate display name and base in their header.
  if (separator >= 0 && !lines.slice(start, separator).some(line => METADATA.test(line))) start = separator + 1;
  let implicits = 0;
  let implicitHeader: number | undefined;
  const result: ItemLine[] = [];
  for (let index = start; index < lines.length; index += 1) {
    const raw = lines[index];
    if (!raw || raw === '--------') continue;
    const count = raw.match(/^Implicits:\s*(\d+)/);
    if (count) { implicits = Number(count[1]); implicitHeader = index; continue; }
    if (METADATA.test(raw)) continue;
    const counted = implicits > 0;
    if (counted) implicits -= 1;
    const namespace = sourceMarker(raw, 'rune') ? 'rune' : sourceMarker(raw, 'enchant') ? 'enchant'
      : counted || sourceMarker(raw, 'implicit') ? 'implicit' : 'explicit';
    result.push({ index, line: cleanLine(raw), namespace, ...(counted ? { implicitHeader } : {}),
      ...(raw.includes('{variant:') ? { uncertain: true } : {}) });
  }
  return result;
}

export function explicitItemLines(text: string): ItemLine[] {
  return itemLines(text).filter(row => row.namespace === 'explicit');
}

function numericValue(line: string, template: StatTemplate, referenceLine = template.line): number | null {
  if (!template.value_indices) return lineValue(line.toLowerCase());
  // A flag has no numeric query value, even when PoB exports a variable roll.
  if (!template.value_indices.length) return 1;
  const numbers = (line.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
  const reference = (referenceLine.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
  if (numbers.length !== reference.length || numbers.some((number, index) =>
    !template.value_indices!.includes(index) && number !== reference[index])) return null;
  return template.value_indices.reduce((sum, index) => sum + numbers[index], 0) / template.value_indices.length;
}

function matchStat(line: string, namespace: string, templates: readonly StatTemplate[]): { id: string; value: number } | undefined {
  // Unresolved rolls cannot be equated with a listing's actual roll.
  if (/\(-?\d+(?:\.\d+)?\s*-\s*-?\d+(?:\.\d+)?\)/.test(line)) return;
  const signature = normalizeTradeLine(line).toLowerCase();
  const matches = templates.flatMap(stat => !stat.id.startsWith(`${namespace}.`) ? [] :
    [stat.line, ...(stat.trade_line ? [stat.trade_line] : [])].filter(candidate =>
      normalizeTradeLine(cleanLine(candidate)).toLowerCase() === signature && numericValue(line, stat, candidate) !== null)
      .map(candidate => ({ stat, candidate })));
  if (new Set(matches.map(({ stat }) => stat.id)).size !== 1) return;
  const { stat: template, candidate } = matches[0];
  const value = numericValue(line, template, candidate);
  const reference = numericValue(candidate, template, candidate);
  if (value === null || !reference) return;
  // Preserve the official sign for inverse wording and average only variable tokens.
  return { id: template.id, value: value * template.value / reference };
}

function matchedItemStats(text: string, namespaces: Set<string>, templates: readonly StatTemplate[]) {
  const lines = itemLines(text).filter(row => namespaces.has(row.namespace) &&
    // Allocated passives have separate trade IDs and are not corruption stat weights.
    !(row.namespace === 'enchant' && /^Allocates /i.test(row.line)));
  const maxLines = Math.max(1, ...templates.map(stat => stat.source_lines?.length ?? 1));
  const groups: { rows: ItemLine[]; line: string; match?: { id: string; value: number } }[] = [];
  for (let index = 0; index < lines.length;) {
    let group = { rows: [lines[index]], line: lines[index].line, match: undefined as { id: string; value: number } | undefined };
    // Prefer the complete multi-line hash over any independently matchable fragment.
    for (let count = Math.min(maxLines, lines.length - index); count >= 1; count -= 1) {
      const rows = lines.slice(index, index + count);
      if (rows.some((row, offset) => row.uncertain || row.namespace !== rows[0].namespace ||
        (offset > 0 && row.index !== rows[offset - 1].index + 1))) continue;
      const line = rows.map(row => row.line).join(' ');
      const match = matchStat(line, rows[0].namespace, templates);
      if (match) { group = { rows, line, match }; break; }
    }
    groups.push(group);
    index += group.rows.length;
  }
  return groups;
}

/** Use the final query's namespaces; base implicits and socket augments remain separate. */
export function scoreEquipment(text: string, weighted: readonly WeightedStat[], templates: readonly StatTemplate[] = weighted): EquipmentScore {
  const queryWeights = tradeQueryWeights(weighted);
  const weights = new Map(queryWeights.map(stat => [stat.id, stat.weight]));
  const namespaces = new Set(['explicit', ...queryWeights.map(stat => stat.id.split('.')[0])]);
  const contributions: ItemScoreContribution[] = [];
  const unscoredLines: string[] = [];
  const groups = matchedItemStats(text, namespaces, templates);
  for (const { line, match } of groups) {
    if (!match) { unscoredLines.push(line); continue; }
    const weight = weights.get(match.id) ?? 0;
    if (weight !== 0) contributions.push({ ...match, line, weight, score: match.value * weight });
  }
  return { score: contributions.reduce((sum, row) => sum + row.score, 0),
    complete: /^\s*Rarity:/im.test(text) && unscoredLines.length === 0, matchedLines: groups.length - unscoredLines.length,
    totalLines: groups.length, contributions, unscoredLines };
}

/** Remove the complete official stat while retaining every other item source. */
export function withoutTradeStat(text: string, stat: StatTemplate, templates: readonly StatTemplate[]): { text: string; value: number } | undefined {
  return tradeStatRemovals(text, templates).get(stat.id);
}

/** Match the current item once, including large option-based passive catalogs. */
export function tradeStatRemovals(text: string, templates: readonly StatTemplate[]): Map<string, { text: string; value: number }> {
  const groups = matchedItemStats(text, new Set(templates.map(stat => stat.id.split('.')[0])), templates);
  const result = new Map<string, { text: string; value: number }>();
  for (const id of new Set(groups.flatMap(group => group.match ? [group.match.id] : []))) {
    const matched = groups.filter(group => group.match?.id === id);
    const removed = new Set(matched.flatMap(group => group.rows.map(row => row.index)));
    const counts = new Map<number, number>();
    for (const row of matched.flatMap(group => group.rows)) {
      if (row.implicitHeader !== undefined) counts.set(row.implicitHeader, (counts.get(row.implicitHeader) ?? 0) + 1);
    }
    result.set(id, { text: text.split('\n').map((line, index) => counts.has(index)
      ? line.replace(/(Implicits:\s*)(\d+)/, (_, prefix: string, count: string) => `${prefix}${Number(count) - counts.get(index)!}`) : line)
      .filter((_, index) => !removed.has(index)).join('\n'),
      value: matched.reduce((sum, group) => sum + group.match!.value, 0) });
  }
  return result;
}

/** Rolled properties already contain local affixes; probe their recalculated values instead. */
export function itemForAffixProbes(text: string): string {
  return text.split('\n').filter(line => !/^(?:Armour|Evasion(?: Rating)?|Energy Shield|Spirit|(?:Runic )?Ward):/.test(line.trim())).join('\n');
}
