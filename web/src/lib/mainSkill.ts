import type { FullDpsResponse, SocketGroupInput } from '../api/types';

/** Use the strongest calculated damage group when an import has no selection.
 * Prefer player groups over equipment-granted utility skills. The caller keeps
 * explicit selections, so this heuristic never changes an author's chosen skill.
 */
export function defaultMainSkill(groups: SocketGroupInput[], report: FullDpsResponse): number | undefined {
  const candidates = report.per_skill.filter(entry => {
    const group = groups[entry.group_index];
    return group?.enabled && group.gems.length > 0 && Number.isFinite(entry.dps) && entry.dps > 0;
  });
  const player = candidates.filter(entry => !groups[entry.group_index].source);
  return (player.length ? player : candidates).sort((a, b) => b.dps - a.dps || a.group_index - b.group_index)[0]?.group_index;
}
