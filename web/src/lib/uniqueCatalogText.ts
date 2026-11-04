import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';
export function uniqueT(lang: Lang, key: keyof Messages['uniqueCatalog']): string {
  return messages(lang, 'uniqueCatalog')[key];
}
