import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';
export function craftT(lang: Lang, key: keyof Messages['itemCrafting']): string {
  return messages(lang, 'itemCrafting')[key];
}
