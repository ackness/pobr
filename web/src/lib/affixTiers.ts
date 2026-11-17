import { normalizeTradeLine } from './trade';
import { cleanLineText } from './modifierText';
import type { TradeAffix } from './tradeOptimizer';

const numbers = (line: string) => [...line.matchAll(/-?\d+(?:\.\d+)?/g)].map(match => Number(match[0]));
const signature = (line: string) => normalizeTradeLine(cleanLineText(line.replace(/\{[^}]*\}/g, ''))
  .replace(/\s*\((?:implicit|enchant|crafted|fractured|rune|tier:\s*\d+)\)/gi, '').trim()).toLocaleLowerCase().replace(/\s+/g, ' ');

export function affixRollLines(mod: TradeAffix, roll = 0.5): string[] {
  if (!mod.roll_lines) throw new Error('missing-ranges');
  if (!Number.isFinite(roll) || roll < 0 || roll > 1) throw new Error('invalid-roll');
  return mod.roll_lines.map(line => line.replace(/\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)/g,
    (_, low: string, high: string) => String(Number((Number(low) + (Number(high) - Number(low)) * roll).toFixed(4)))));
}

export function affixFamily(mod: TradeAffix, pool: readonly TradeAffix[]): TradeAffix[] {
  const key = mod.lines.map(signature).sort().join('\n');
  const magnitude = (candidate: TradeAffix) => (candidate.roll_lines ? affixRollLines(candidate, 1) : candidate.lines)
    .flatMap(numbers).reduce((sum, value) => sum + Math.abs(value), 0);
  return pool.filter(candidate => candidate.domain === mod.domain && candidate.group === mod.group && candidate.kind === mod.kind
    && candidate.lines.map(signature).sort().join('\n') === key)
    .sort((first, second) => second.level - first.level || magnitude(second) - magnitude(first) || second.id.localeCompare(first.id));
}

export function affixTier(mod: TradeAffix, pool: readonly TradeAffix[]): number {
  return affixFamily(mod, pool).findIndex(candidate => candidate.id === mod.id) + 1;
}
