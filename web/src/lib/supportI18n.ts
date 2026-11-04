import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';

export function supportText(lang: Lang, key: keyof Messages['support']): string {
  return messages(lang, 'support')[key];
}
