import type { CalculateBuildResponse, MinionOutput } from '../api/types';
import { bindT, type Lang } from './i18n';

/**
 * The display stat the engine adds while the calculated skill summons a minion: the hit DPS of
 * one such minion. The engine knows no live minion count, only the limit, so nothing multiplies it.
 * The what-if endpoints (tree node power, variants, source contributions) accept it as a stat id.
 */
export const MAIN_MINION_DPS = 'MainMinionDPS';

type MinionFields = Pick<CalculateBuildResponse, 'minions' | 'main_minion_index'>;

/** The minion the calculated skill summons; its damage is a summoner build's headline. */
export function mainMinion(calc: MinionFields | null | undefined): MinionOutput | undefined {
  if (calc?.main_minion_index == null) return undefined;
  return calc.minions?.find(minion => minion.index === calc.main_minion_index);
}

/** The damage stat a build is judged by: the main minion's for a summoner build, otherwise the player's combined DPS. */
export function headlineDpsStat(calc: MinionFields | null | undefined): string {
  return mainMinion(calc) ? MAIN_MINION_DPS : 'CombinedDPS';
}

/** "Wolf · one minion · limit 7": the headline is one minion's damage, never multiplied by the limit. */
export function minionDpsNote(lang: Lang, name: string, limit: number | null | undefined): string {
  const tt = bindT(lang);
  const parts = [name, tt('minion.perMinion')];
  if (typeof limit === 'number' && limit > 0) parts.push(tt('minion.limit').replace('{count}', String(Math.round(limit))));
  return parts.join(' · ');
}
