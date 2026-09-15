import { lineValue, normalizeTradeLine, tradeQueryWeights, type WeightedStat } from './trade';

export interface ItemScoreContribution {
  id: string;
  line: string;
  value: number;
  weight: number;
  score: number;
}

export interface EquipmentScore {
  /** The official query's weighted explicit-stat Sum, never a DPS estimate. */
  score: number;
  /** False when an explicit line could not be mapped without guessing. */
  complete: boolean;
  matchedLines: number;
  totalLines: number;
  contributions: ItemScoreContribution[];
  unscoredLines: string[];
}

type StatTemplate = Pick<WeightedStat, 'id' | 'line' | 'value'>;
interface ExplicitLine { index: number; line: string; uncertain?: boolean }

const METADATA = /^(?:Item Class|Rarity|Item Level|Requirements|Level|LevelReq|Str|Dex|Int|Sockets|Rune|Soul Core|Charm Slots|Armour|Evasion(?: Rating)?|Energy Shield|Spirit|Ward|Block|Quality|Physical Damage|Elemental Damage|Chaos Damage|Critical Hit Chance|Attacks per Second|Weapon Range|Unique ID|Item ID|Note|Selected Variant|Variant|Radius|Talisman Tier|Limited to|Crafted|Prefix|Suffix|Catalyst|CatalystQuality):|^(?:Corrupted|Mirrored|Unidentified|Has Alt Variant|Requires\b)/;
const NON_EXPLICIT = /\{(?:crafted|enchant|rune|implicit)\}|\((?:implicit|enchant|crafted|rune)\)/i;

function cleanLine(line: string): string {
  return line.replace(/\{[^}]*\}/g, '').replace(/\s*\((?:[a-z]+|tier:\s*\d+)\)/gi, '')
    .replace(/\s*\[(?:augmented|crafted|fractured)\]/gi, '').trim();
}

/** Preserve original indices so an actual affix can be removed for a marginal probe. */
export function explicitItemLines(text: string): ExplicitLine[] {
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
  const result: ExplicitLine[] = [];
  for (let index = start; index < lines.length; index += 1) {
    const raw = lines[index];
    if (!raw || raw === '--------') continue;
    const count = raw.match(/^Implicits:\s*(\d+)/);
    if (count) { implicits = Number(count[1]); continue; }
    if (METADATA.test(raw)) continue;
    // PoB's count includes rune and enchant lines as well as ordinary implicits.
    if (implicits > 0) { implicits -= 1; continue; }
    if (NON_EXPLICIT.test(raw)) continue;
    result.push({ index, line: cleanLine(raw), ...(raw.includes('{variant:') ? { uncertain: true } : {}) });
  }
  return result;
}

function matchStat(line: string, templates: readonly StatTemplate[]): { id: string; value: number } | undefined {
  // Unresolved rolls and variant annotations cannot be equated with a listing's actual roll.
  if (/\(-?\d+(?:\.\d+)?\s*-\s*-?\d+(?:\.\d+)?\)/.test(line)) return;
  const signature = normalizeTradeLine(line).toLowerCase();
  const matches = templates.filter(stat => normalizeTradeLine(cleanLine(stat.line)).toLowerCase() === signature);
  if (new Set(matches.map(stat => stat.id)).size !== 1) return;
  const template = matches[0];
  const value = lineValue(line.toLowerCase());
  const templateValue = lineValue(template.line.toLowerCase());
  if (value === null || !templateValue) return;
  // The catalog already accounts for trade stats expressed as the inverse effect.
  return { id: template.id, value: value * template.value / templateValue };
}

/** Score only stats present in the final query; implicits/augments/base properties are separate. */
export function scoreEquipment(text: string, weighted: readonly WeightedStat[], templates: readonly StatTemplate[] = weighted): EquipmentScore {
  const weights = new Map(tradeQueryWeights(weighted).map(stat => [stat.id, stat.weight]));
  const contributions: ItemScoreContribution[] = [];
  const unscoredLines: string[] = [];
  const lines = explicitItemLines(text);
  for (const { line, uncertain } of lines) {
    const match = uncertain ? undefined : matchStat(line, templates);
    if (!match) { unscoredLines.push(line); continue; }
    const weight = weights.get(match.id) ?? 0;
    if (weight !== 0) contributions.push({ ...match, line, weight, score: match.value * weight });
  }
  return { score: contributions.reduce((sum, row) => sum + row.score, 0),
    complete: /^\s*Rarity:/im.test(text) && unscoredLines.length === 0, matchedLines: lines.length - unscoredLines.length,
    totalLines: lines.length, contributions, unscoredLines };
}

/** Remove matching explicit lines, retaining the item's implicit and every other source. */
export function withoutTradeStat(text: string, stat: StatTemplate, templates: readonly StatTemplate[]): { text: string; value: number } | undefined {
  const matched = explicitItemLines(text).flatMap(row => {
    const match = row.uncertain ? undefined : matchStat(row.line, templates);
    return match?.id === stat.id ? [{ ...row, value: match.value }] : [];
  });
  if (!matched.length) return;
  const removed = new Set(matched.map(row => row.index));
  return { text: text.split('\n').filter((_, index) => !removed.has(index)).join('\n'),
    value: matched.reduce((sum, row) => sum + row.value, 0) };
}

/** Rolled properties already contain local affixes; probe their recalculated values instead. */
export function itemForAffixProbes(text: string): string {
  return text.split('\n').filter(line => !/^(?:Armour|Evasion(?: Rating)?|Energy Shield|Spirit|Ward):/.test(line.trim())).join('\n');
}
