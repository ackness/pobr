import type { CalculateBuildResponse } from '../api/types';
import type { Route } from './routes';

export type Severity = 'bad' | 'warn';
export type Finding =
  | { kind: 'no-main-skill'; severity: Severity; route: Route }
  | { kind: 'item-errors'; severity: Severity; route: Route; slots: string[] }
  | { kind: 'spirit'; severity: Severity; route: Route; value: number }
  | { kind: 'attribute'; severity: Severity; route: Route; attribute: 'Str' | 'Dex' | 'Int'; have: number; need: number }
  | { kind: 'resist'; severity: Severity; route: Route; element: 'Fire' | 'Cold' | 'Lightning' | 'Chaos'; value: number; missing: number }
  | { kind: 'hit-chance'; severity: Severity; route: Route; value: number }
  | { kind: 'net-regen'; severity: Severity; route: Route; value: number }
  | { kind: 'unsupported'; severity: Severity; route: Route; count: number };

/** The elemental resistance cap a character has before any maximum-resistance modifier. */
export const BASE_RESIST_CAP = 75;
const ELEMENTS = ['Fire', 'Cold', 'Lightning'] as const;
const ATTRIBUTES = ['Str', 'Dex', 'Int'] as const;

/**
 * What a player importing someone else's build should look at first, read off the numbers the
 * engine already returned. Nothing here recalculates; each rule only compares two outputs, so a
 * finding always points at a value the player can open in Calculations.
 */
export function deriveFindings(calc: CalculateBuildResponse | null, hasSkills: boolean): Finding[] {
  if (!calc) return [];
  const stat = new Map(calc.stats.map(entry => [entry.id, entry.value] as const));
  const num = (id: string) => { const value = stat.get(id); return typeof value === 'number' && Number.isFinite(value) ? value : null; };
  const found: Finding[] = [];

  if (hasSkills && !calc.main_skill) found.push({ kind: 'no-main-skill', severity: 'bad', route: 'skills' });
  if (calc.item_errors.length) found.push({ kind: 'item-errors', severity: 'bad', route: 'items', slots: calc.item_errors.map(issue => issue.slot) });

  const spirit = num('SpiritUnreserved');
  if (spirit !== null && spirit < 0) found.push({ kind: 'spirit', severity: 'bad', route: 'skills', value: spirit });

  for (const attribute of ATTRIBUTES) {
    const have = num(attribute), need = num(`Req${attribute}`);
    if (have !== null && need !== null && need > have) found.push({ kind: 'attribute', severity: 'bad', route: 'items', attribute, have, need });
  }
  for (const element of ELEMENTS) {
    const value = num(`${element}Resist`), over = num(`${element}ResistOverCap`) ?? 0;
    // A value under the base cap with nothing over a (possibly raised) maximum is a real shortfall.
    if (value !== null && value < BASE_RESIST_CAP && over <= 0) found.push({ kind: 'resist', severity: value < 0 ? 'bad' : 'warn', route: 'items', element, value, missing: BASE_RESIST_CAP - value });
  }
  const chaos = num('ChaosResist');
  if (chaos !== null && chaos < 0) found.push({ kind: 'resist', severity: 'warn', route: 'items', element: 'Chaos', value: chaos, missing: -chaos });

  const hit = num('HitChance');
  if (hit !== null && hit > 0 && hit < 95) found.push({ kind: 'hit-chance', severity: hit < 80 ? 'bad' : 'warn', route: 'items', value: hit });
  const net = num('TotalNetRegen');
  if (net !== null && net < 0) found.push({ kind: 'net-regen', severity: 'warn', route: 'calcs', value: net });
  if (calc.unsupported_modifiers.length) found.push({ kind: 'unsupported', severity: 'warn', route: 'calcs', count: calc.unsupported_modifiers.length });

  return found.sort((a, b) => Number(b.severity === 'bad') - Number(a.severity === 'bad'));
}
