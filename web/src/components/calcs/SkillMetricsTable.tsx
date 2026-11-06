import type { SkillMetricsEntry, SkillMetricsResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { bindT, grantedSourceLabel, type Lang } from '../../lib/i18n';
import { mainMinion } from '../../lib/mainMinion';
import { highestDpsSkill, isNonDamagingSkill, isSummonerSkill } from '../../lib/skillSelection';
import { formatStatValue } from '../../lib/statDisplay';
import { skillDisplayName } from '../../lib/skillNames';
import { AppButton } from '../shared/controls';

export { isNonDamagingSkill, isSummonerSkill } from '../../lib/skillSelection';

export function SkillMetricsTable({ session, lang, report, displayed }: {
  session: BuildSession; lang: Lang; report: SkillMetricsResponse | null; displayed: SkillMetricsResponse;
}) {
  const tt = bindT(lang);
  const minion = mainMinion(session.calc);
  const calculated = session.calc?.main_skill;
  const catalog = useSkillCatalog();
  const skillName = (id: string) => skillDisplayName(id, lang, catalog);
  const mainIndex = session.calcParams.main_socket_group;
  // The calculated effect's damage is its main minion's when it has one, as on the rail and Overview.
  const minionDpsOf = (entry: SkillMetricsEntry) => minion && calculated?.group_index === entry.group_index
    && calculated.active_skill_index === entry.active_skill_index ? minion.dps : null;
  const dpsOf = (entry: SkillMetricsEntry) => minionDpsOf(entry) ?? entry.metrics!.combined_dps;
  const highest = report ? highestDpsSkill(report.skills, session.socketGroups, dpsOf) : null;
  const unsupported = [...new Set(displayed.skills.flatMap(entry => entry.unsupported_modifiers))];
  const nameCounts = new Map<string, number>();
  for (const entry of displayed.skills) {
    const name = skillName(entry.skill_id);
    nameCounts.set(name, (nameCounts.get(name) ?? 0) + 1);
  }
  const columns = [
    ['combined_dps', 'sidebar.combinedDps', 'float2'],
    ['average_hit', 'calcs.averageHit', 'float2'],
    ['action_rate', 'calcs.actionRate', 'rate'],
    ['mana_cost', 'calcs.manaCost', 'int'],
    ['life_cost', 'calcs.lifeCost', 'int'],
    ['cooldown', 'calcs.cooldown', 'float2'],
  ] as const;
  return <>
    {unsupported.length > 0 && <details className="skill-metrics-limits">
      <summary>{tt('calcs.unsupported')}</summary>
      <ul>{unsupported.map(modifier => <li key={modifier}>{modifier}</li>)}</ul>
    </details>}
    <div className="breakdown-scroll">
      <table className="breakdown-table skill-metrics-table">
        <thead><tr>
          <th scope="col">{tt('calcs.skill')}</th>
          {columns.map(([key, label]) => <th scope="col" key={key}>{tt(label)}{key === 'combined_dps' && minion && <> / {tt('sidebar.minionHitDps')}</>}</th>)}
          <th scope="col">{tt('calcs.diagnostics')}</th>
        </tr></thead>
        <tbody>{displayed.skills.map(entry => {
          const group = session.socketGroups[entry.group_index];
          const granted = grantedSourceLabel(lang, group?.source);
          const selected = entry.group_index === mainIndex && entry.active_skill_index === (group?.main_active_skill ?? 1);
          const available = !!report && entry.status === 'calculated' && entry.metrics !== null;
          const nonDamaging = isNonDamagingSkill(entry, catalog);
          // A summoning skill deals its damage through minions; only the calculated one's minion is calculated.
          const summoner = available && isSummonerSkill(entry, catalog);
          const minionDps = available ? minionDpsOf(entry) : null;
          const disabled = !report || session.busy || !group?.enabled;
          const name = skillName(entry.skill_id);
          const label = nameCounts.get(name)! > 1 ? tt('calcs.skillInstance')
            .replace('{skill}', name).replace('{group}', String(entry.group_index + 1))
            .replace('{effect}', String(entry.active_skill_index)) : name;
          const inspect = () => {
            if (!disabled) session.inspectSkill(entry.group_index, entry.active_skill_index, session.stateVersion);
          };
          return <tr key={`${entry.group_index}-${entry.active_skill_index}`}
            className={`skill-metrics-row${selected ? ' is-main' : ''}${granted ? ' is-granted' : ''}`} onClick={inspect}>
            <td>
              <AppButton className="calcs-skill-button h-auto justify-start whitespace-normal text-left" aria-pressed={selected}
                title={group?.gems.map(gem => skillName(gem.skill_id)).join(' + ')} aria-disabled={disabled}
                onClick={event => { event.stopPropagation(); inspect(); }}>
                {label}
              </AppButton>
              {granted && <span className="granted-badge">{granted}</span>}
              {nonDamaging && <span className="granted-badge">{tt('calcs.nonDamaging')}</span>}
              {minionDps !== null && <span className="granted-badge">{tt('skills.minionDps')}</span>}
              {entry === highest && dpsOf(entry) > 0 && <span className="granted-badge">{tt(minionDps !== null ? 'calcs.highestMinionDps' : 'calcs.highestDps')}</span>}
            </td>
            {columns.map(([key, , format]) => <td className="mod-value" key={key}>
              {key === 'combined_dps' && minionDps !== null ? formatStatValue(minionDps, 'float2', lang)
                : available && !((nonDamaging || summoner) && (key === 'combined_dps' || key === 'average_hit'))
                  ? formatStatValue(entry.metrics![key], format, lang) : '—'}
            </td>)}
            <td className="skill-metrics-diagnostics">
              {report && !available && <p>{tt('calcs.unavailable')}</p>}
              {summoner && minionDps === null && <p>{tt('skills.summonerPending')}</p>}
              {report && available && entry.unsupported_modifiers.length > 0 && <p>{tt('calcs.excludedModifiers')}</p>}
              {report && entry.error && <details onClick={event => event.stopPropagation()}>
                <summary>{tt('calcs.errorDetails')}</summary><p>{entry.error}</p>
              </details>}
            </td>
          </tr>;
        })}</tbody>
      </table>
    </div>
  </>;
}
