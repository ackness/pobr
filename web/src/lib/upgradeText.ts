import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';

export function upgradeT(lang: Lang, key: keyof Messages['upgrade']): string {
  return messages(lang, 'upgrade')[key];
}
