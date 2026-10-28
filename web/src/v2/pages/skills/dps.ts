import type { SkillMetricsEntry } from '../../../api/types';

/**
 * What a socket group's selected skill does on its own. `damage` carries a figure, `none`
 * is a skill that calculates fine but deals no damage (a curse, a buff, a reservation), and
 * `unavailable` means no figure exists yet or the engine could not calculate that effect.
 */
export type GroupDps =
  | { kind: 'damage'; dps: number }
  | { kind: 'none' }
  | { kind: 'unavailable' };

/** The slice of a socket group this module reads: which of its active effects is selected. */
export interface GroupSelection { main_active_skill?: number | null }

/**
 * One entry per group, keyed by group index. A group is measured by the effect it has
 * selected (`main_active_skill`, 1-based, default 1), the same effect the main calculation uses.
 */
export function groupDps(skills: readonly SkillMetricsEntry[], groups: readonly GroupSelection[]): Map<number, GroupDps> {
  const byEffect = new Map<string, SkillMetricsEntry>();
  for (const entry of skills) byEffect.set(`${entry.group_index}:${entry.active_skill_index}`, entry);
  const rows = new Map<number, GroupDps>();
  groups.forEach((group, index) => {
    const entry = byEffect.get(`${index}:${group.main_active_skill ?? 1}`);
    const metrics = entry?.status === 'calculated' ? entry.metrics : null;
    if (!metrics || !Number.isFinite(metrics.combined_dps)) rows.set(index, { kind: 'unavailable' });
    else rows.set(index, metrics.combined_dps > 0 ? { kind: 'damage', dps: metrics.combined_dps } : { kind: 'none' });
  });
  return rows;
}

/** The largest figure in the list: every bar is drawn relative to it. */
export function topDps(rows: ReadonlyMap<number, GroupDps>): number {
  let top = 0;
  for (const row of rows.values()) if (row.kind === 'damage' && row.dps > top) top = row.dps;
  return top;
}

/** Bar length as a fraction in [0, 1]; a skill that deals damage always shows a visible sliver. */
export function barShare(dps: number, top: number): number {
  if (!(top > 0) || !(dps > 0)) return 0;
  return Math.min(1, Math.max(0.02, dps / top));
}
