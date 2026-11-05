import { bindT, slotLabel, type Lang } from './i18n';

/** Attribution reports full-build minus removed-source values. Display the removal itself. */
export function removalChange(delta: number): number {
  return -delta;
}

export function contributionLabel(lang: Lang, kind: string, id: string): string {
  const tt = bindT(lang);
  if (kind === 'socket_group') return tt('calcs.sourceGroup').replace('{number}', String(Number(id) + 1));
  return `${tt('calcs.sourceEquipment')} · ${slotLabel(lang, id)}`;
}
