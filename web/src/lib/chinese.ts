import { Converter as traditionalConverter } from 'opencc-js/cn2t';
import { Converter as simplifiedConverter } from 'opencc-js/t2cn';
import type { Lang } from './i18n';

// Script conversion is a display fallback, not an official game translation.
// Callers must prefer the requested locale's game data before using this.
const traditional = traditionalConverter({ from: 'cn', to: 'tw' });
const simplified = simplifiedConverter({ from: 'tw', to: 'cn' });

export function chineseScript(text: string, lang: Lang): string {
  if (lang === 'zh-TW') return traditional(text);
  if (lang === 'zh-CN') return simplified(text);
  return text;
}

export function localizedName(
  lang: Lang,
  english: string,
  zhCN?: string | null,
  zhTW?: string | null,
): string {
  if (lang === 'en-US') return english;
  const preferred = lang === 'zh-CN' ? zhCN : zhTW;
  if (preferred?.trim()) return preferred.trim();
  const alternate = lang === 'zh-CN' ? zhTW : zhCN;
  return alternate?.trim() ? chineseScript(alternate.trim(), lang) : english;
}
