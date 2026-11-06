import { useMemo } from 'react';
import type { CalculateBuildResponse, MinionOutput } from '../api/types';
import { bindT, type Lang } from '../lib/i18n';
import { headlineDpsStat, mainMinion, minionDpsNote, type DamageHeadline } from '../lib/mainMinion';
import { useLocalizedNames } from './useLocalizedLines';

/** The calculated skill's minion with its game name in the current language (English until the name arrives, or without a record). */
export function useMainMinion(calc: CalculateBuildResponse | null | undefined, lang: Lang): { minion: MinionOutput; name: string } | null {
  const minion = mainMinion(calc);
  const variety = minion?.monster_variety_id, english = minion?.name;
  const names = useLocalizedNames(useMemo(() => english !== undefined ? [{ kind: 'monster' as const, id: variety, name: english }] : [],
    [variety, english]), lang);
  if (!minion) return null;
  return { minion, name: names[0] || minion.name || bindT(lang)('minion.title') };
}

/** Planner headlines describe the same damage stat used by the shared damage goal. */
export function useHeadlineDamage(calc: CalculateBuildResponse | null | undefined, lang: Lang): DamageHeadline {
  const minion = useMainMinion(calc, lang);
  return {
    stat: headlineDpsStat(calc),
    label: bindT(lang)(minion ? 'sidebar.minionHitDps' : 'sidebar.combinedDps'),
    note: minion ? minionDpsNote(lang, minion.name, calc?.stats?.find(stat => stat.id === 'ActiveMinionLimit')?.value) : undefined,
  };
}
