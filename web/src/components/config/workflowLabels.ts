import { messages, type Lang } from '../../lib/i18n';
import type { Messages } from '../../locales';

export function workflowLabel(lang: Lang, key: keyof Messages['configWorkflow']): string {
  return messages(lang, 'configWorkflow')[key];
}
