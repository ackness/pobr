import terms from './configGameTerms.json';
import { chineseScript } from './chinese';
import type { Lang } from './i18n';

// Selected game names used by configuration prose, paired by stable skill ID
// from the gem catalog fixture. The coverage test checks their provenance.
// Other prose is authored app copy, with OpenCC as a script fallback.
const traditionalNames = new Map(terms.map(term => [term.cn, term.tw]));
const escape = (text: string): string => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const pattern = new RegExp(`(${[...traditionalNames.keys()].sort((a, b) => b.length - a.length).map(escape).join('|')})`, 'g');

export function configCopy(text: string, lang: Lang): string {
  if (lang !== 'zh-TW') return chineseScript(text, lang);
  return text.split(pattern).map(part => traditionalNames.get(part) ?? chineseScript(part, lang)).join('');
}
