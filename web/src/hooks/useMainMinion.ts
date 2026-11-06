import { useMemo } from 'react';
import type { CalculateBuildResponse, MinionOutput } from '../api/types';
import { bindT, type Lang } from '../lib/i18n';
import { mainMinion } from '../lib/mainMinion';
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
