import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';

export function affixT(lang: Lang, key: keyof Messages['marketAffix']): string {
  return messages(lang, 'marketAffix')[key];
}
