import { Converter as traditionalConverter } from 'opencc-js/cn2t';
import { Converter as simplifiedConverter } from 'opencc-js/t2cn';
import { gameTextChain } from './gameLocales';
import type { Lang } from './i18n';

// Script conversion is a display fallback, not an official game translation.
// Callers must prefer the requested locale's game data before using this.
const traditional = traditionalConverter({ from: 'cn', to: 'tw' });
const simplified = simplifiedConverter({ from: 'tw', to: 'cn' });

/** Writes `text` in `lang`'s script where a converter exists; other languages keep it as is. */
export function chineseScript(text: string, lang: Lang): string {
  if (lang === 'zh-TW') return traditional(text);
  if (lang === 'zh-CN') return simplified(text);
  return text;
}

/**
 * The display name in `lang`: its own name, else the first name its game-text
 * fallback chain has (written in `lang`'s script), else the English one.
 */
export function localizedName(
  lang: Lang,
  english: string,
  zhCN?: string | null,
  zhTW?: string | null,
): string {
  const names: Readonly<Record<string, string | null | undefined>> = { 'zh-CN': zhCN, 'zh-TW': zhTW };
  for (const code of gameTextChain(lang)) {
    const name = names[code]?.trim();
    if (name) return code === lang ? name : chineseScript(name, lang);
  }
  return english;
}
