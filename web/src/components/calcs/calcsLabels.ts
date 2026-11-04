import { messages, type Lang } from '../../lib/i18n';
import type { Messages } from '../../locales';

export function calcsLabel(lang: Lang, key: keyof Messages['calcs']): string {
  return messages(lang, 'calcs')[key];
}
