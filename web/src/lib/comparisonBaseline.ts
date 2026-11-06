import type { Lang } from './i18n';

export function baselineTime(recordedAt: number, lang: Lang): string {
  return new Intl.DateTimeFormat(lang, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(recordedAt);
}
