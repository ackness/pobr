import { messages, type Lang } from '../lib/i18n';
export function words(lang: Lang, en: string, cn: string, tw: string = cn) { return lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn; }
/** Short page names for document titles and display settings. */
export const pageNames = (lang: Lang) => messages(lang, 'pageName');
