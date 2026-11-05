import { bindT, type Lang } from '../lib/i18n';

export function calculationVersion(lang: Lang, data: string | null | undefined, engine: string | null | undefined, time: number | null): string {
  const tt = bindT(lang);
  return [
    data && tt('shell.dataVersion').replace('{value}', data),
    engine && tt('shell.engineVersion').replace('{value}', engine),
    time !== null && tt('shell.calculationTime').replace('{value}', String(Math.max(1, Math.round(time)))),
  ].filter(Boolean).join(' · ');
}
