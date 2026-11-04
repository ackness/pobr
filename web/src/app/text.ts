import { messages, type Lang } from '../lib/i18n';
export function words(lang: Lang, en: string, cn: string, tw: string = cn) { return lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn; }
/** Document titles; the navigation itself uses the shorter `tab.*` labels. */
export const pageNames = (lang: Lang) => messages(lang, 'pageName');
