import type { CalculateBuildResponse, GemCatalogEntry, SocketGroupInput, UnmodelledMetaTrigger } from '../../api/types';
import { hasEnergyTrigger } from '../../lib/configEncounter';
import { bindT, type Lang } from '../../lib/i18n';
import { unmodelledMetaTriggerNote } from '../../lib/triggerNotes';

type Props = { lang: Lang; className?: string } & (
  | { trigger: UnmodelledMetaTrigger | undefined; catalog: ReadonlyMap<string, GemCatalogEntry>; calc?: never; groups?: never }
  | { calc: CalculateBuildResponse | null | undefined; groups?: readonly SocketGroupInput[]; trigger?: never; catalog?: never }
);

export function EnergyTriggerNote({ calc, groups, trigger, catalog, lang, className = '' }: Props) {
  const note = trigger && catalog ? unmodelledMetaTriggerNote(trigger, lang, catalog)
    : hasEnergyTrigger(calc, groups) ? bindT(lang)('config.energyMappingNote') : null;
  return note ? <small className={`energy-trigger-note text-muted-foreground block max-w-prose text-xs leading-relaxed ${className}`}>
    {note}
  </small> : null;
}
