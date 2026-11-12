import { gameTextFallback, SOURCE_LANG, type Lang } from '../locales';

/** One locale the data pack declares (WASM `availableLocales()`). */
export type GameLocaleInfo = import('../api/generated/dataInfo').LocaleInfo;

let declared = new Map<string, GameLocaleInfo>();

/** Record the loaded data pack's locales (replaces any earlier pack's). */
export function setGameLocales(locales: readonly GameLocaleInfo[]): void {
  declared = new Map(locales.map(locale => [locale.code, locale]));
}

/**
 * The locales whose game text answers for `lang`, in order: `lang`, the
 * fallbacks the data pack declares for it, then the interface file's
 * `language.gameTextFallback` entries (each followed by its own declared
 * fallbacks). English is the implicit end and is not listed. Before a data pack
 * is loaded, only the interface file's entries are known. The interface-copy
 * fallback (`language.fallback`) plays no part.
 */
export function gameTextChain(lang: Lang): readonly string[] {
  if (lang === SOURCE_LANG) return [];
  const extra = gameTextFallback(lang).filter(code => code !== lang && code !== SOURCE_LANG);
  const seen = new Set<string>();
  const chain: string[] = [];
  const pending = [...extra].reverse();
  pending.push(lang);
  while (pending.length) {
    const code = pending.pop()!;
    if (seen.has(code) || code === SOURCE_LANG) continue;
    seen.add(code);
    chain.push(code);
    pending.push(...[...(declared.get(code)?.fallback ?? [])].reverse());
  }
  return chain;
}

/** The first locale in `lang`'s chain whose pack has mod-line templates, if any. */
export function lineTemplateLocale(lang: Lang): string | undefined {
  return gameTextChain(lang).find(code => declared.get(code)?.templates);
}
