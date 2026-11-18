import { messages, t, type Lang } from '../lib/i18n';
export const colon = (lang: Lang) => t(lang, 'format.colon');
/** Short page names for document titles and display settings. */
export const pageNames = (lang: Lang) => messages(lang, 'pageName');
