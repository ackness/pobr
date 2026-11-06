import type { CalculateBuildResponse, SocketGroupInput } from '../../api/types';
import { hasEnergyTrigger } from '../../lib/configEncounter';
import { bindT, type Lang } from '../../lib/i18n';

export function EnergyTriggerNote({ calc, groups, lang, className = '' }: { calc: CalculateBuildResponse | null | undefined; groups?: readonly SocketGroupInput[]; lang: Lang; className?: string }) {
  return hasEnergyTrigger(calc, groups) ? <small className={`energy-trigger-note text-muted-foreground block max-w-prose text-xs leading-relaxed ${className}`}>
    {bindT(lang)('config.energyMappingNote')}
  </small> : null;
}
