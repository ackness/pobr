import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';

export function guidanceT(lang: Lang, key: keyof Messages['guidance']): string {
  return messages(lang, 'guidance')[key];
}
