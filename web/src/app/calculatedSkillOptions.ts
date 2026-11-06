import type { CalculateBuildResponse, GemCatalogEntry, SkillMetricsEntry, SocketGroupInput } from '../api/types';
import type { AppSelectOption } from '../components/shared/AppSelect';
import { bindT, type Lang } from '../lib/i18n';
import { mainMinion } from '../lib/mainMinion';
import { activeSkillEffects, isNonDamagingSkill, isSummonerSkill } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { formatStatValue } from '../lib/statDisplay';

export function calculatedSkillOptions(groups: SocketGroupInput[], catalog: ReadonlyMap<string, GemCatalogEntry>,
  skills: readonly SkillMetricsEntry[], calc: CalculateBuildResponse | null, lang: Lang): AppSelectOption[] {
  const tt = bindT(lang);
  const minion = mainMinion(calc);
  return groups.map((group, index) => {
    const ordinal = group.main_active_skill ?? 1;
    const effect = activeSkillEffects(group, catalog)[ordinal - 1];
    const name = effect ? skillDisplayName(effect.skillId, lang, catalog)
      : group.gems.find(gem => gem.name_spec)?.name_spec || tt('skills.emptyGroup');
    const measured = skills.find(entry => entry.group_index === index && entry.active_skill_index === ordinal);
    const current = calc?.main_skill?.group_index === index && calc.main_skill.active_skill_index === ordinal;
    const dps = current && minion ? minion.dps : current ? calc!.main_skill!.combined_dps : measured?.metrics?.combined_dps;
    const summoner = effect && catalog.get(group.gems[effect.gemIndex].skill_id)?.tags.includes('CreatesMinion');
    const nonDamaging = !!measured && !summoner && isNonDamagingSkill(measured, catalog);
    const label = !effect || measured?.status === 'unavailable' ? tt('sidebar.skillUnsupported').replace('{name}', name) : name;
    const pendingSummoner = (dps ?? 0) === 0 && !(current && minion)
      && (measured ? isSummonerSkill(measured, catalog) : summoner);
    const damage = pendingSummoner ? tt('skills.summonerPending') : dps !== undefined && (current || measured?.status === 'calculated')
      ? `${formatStatValue(dps, 'float2', lang)} ${tt(current && minion ? 'sidebar.minionHitDps' : 'sidebar.combinedDps')}`
      : summoner ? tt('skills.summonerPending') : tt('sidebar.skillDpsPending');
    return { value: String(index), label: `${label} · ${damage} · ${tt('sidebar.skillGroupNumber').replace('{number}', String(index + 1))}${group.enabled ? '' : ` ${tt('sidebar.disabledGroup')}`}`,
      group: tt(nonDamaging ? 'sidebar.nonDamagingSkills' : 'sidebar.calculationSkills') };
  }).sort((a, b) => Number(a.group === tt('sidebar.nonDamagingSkills')) - Number(b.group === tt('sidebar.nonDamagingSkills')));
}
