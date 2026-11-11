import type { SkillMetricsEntry, UnmodelledMetaTrigger } from '../../../api/types';

/**
 * What a socket group's selected skill does on its own. `damage` carries a figure, `none`
 * is a skill that calculates fine but deals no damage (a curse, a buff, a reservation), and
 * `unavailable` means no figure exists yet or the engine could not calculate that effect.
 * An unmodelled meta trigger keeps its zero figure and the explanation attached to it.
 * A summoning skill deals its damage through minions: `minion` carries one minion's hit DPS
 * when it is the calculated skill, and `summoner` marks one whose minions are not calculated.
 */
export type GroupDps =
  | { kind: 'damage'; dps: number; unmodelled_meta_trigger?: UnmodelledMetaTrigger }
  | { kind: 'minion'; dps: number }
  | { kind: 'summoner' }
  | { kind: 'none' }
  | { kind: 'unavailable' };

/** The slice of a socket group this module reads: which of its active effects is selected. */
export interface GroupSelection { main_active_skill?: number | null }

/** The calculated effect when it summons the main minion, and that minion's hit DPS. */
export interface CalculatedMinion { group_index: number; active_skill_index: number; dps: number }

/**
 * One entry per group, keyed by group index. A group is measured by the effect it has
 * selected (`main_active_skill`, 1-based, default 1), the same effect the main calculation uses.
 * `isSummoner` tells a summoning skill (no damage of its own) from one that deals none at all.
 */
export function groupDps(skills: readonly SkillMetricsEntry[], groups: readonly GroupSelection[],
  minion?: CalculatedMinion | null, isSummoner: (skillId: string) => boolean = () => false): Map<number, GroupDps> {
  const byEffect = new Map<string, SkillMetricsEntry>();
  for (const entry of skills) byEffect.set(`${entry.group_index}:${entry.active_skill_index}`, entry);
  const rows = new Map<number, GroupDps>();
  groups.forEach((group, index) => {
    const effect = group.main_active_skill ?? 1;
    const entry = byEffect.get(`${index}:${effect}`);
    const metrics = entry?.status === 'calculated' ? entry.metrics : null;
    if (minion && minion.group_index === index && minion.active_skill_index === effect && Number.isFinite(minion.dps))
      rows.set(index, { kind: 'minion', dps: minion.dps });
    else if (!metrics || !Number.isFinite(metrics.combined_dps)) rows.set(index, { kind: 'unavailable' });
    else if (entry?.unmodelled_meta_trigger) rows.set(index, { kind: 'damage', dps: metrics.combined_dps, unmodelled_meta_trigger: entry.unmodelled_meta_trigger });
    else if (metrics.combined_dps > 0) rows.set(index, { kind: 'damage', dps: metrics.combined_dps });
    else rows.set(index, isSummoner(entry!.skill_id) ? { kind: 'summoner' } : { kind: 'none' });
  });
  return rows;
}

/** The largest figure in the list: every bar is drawn relative to it. */
export function topDps(rows: ReadonlyMap<number, GroupDps>): number {
  let top = 0;
  for (const row of rows.values()) if ((row.kind === 'damage' || row.kind === 'minion') && row.dps > top) top = row.dps;
  return top;
}

/** Bar length as a fraction in [0, 1]; a skill that deals damage always shows a visible sliver. */
export function barShare(dps: number, top: number): number {
  if (!(top > 0) || !(dps > 0)) return 0;
  return Math.min(1, Math.max(0.02, dps / top));
}
