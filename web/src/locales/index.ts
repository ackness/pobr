/**
 * Interface copy, one JSON file per language. Game names and modifier text
 * come from the versioned localization data instead.
 *
 * en-US.json is the complete key set, and `mise run i18n:check` requires
 * every registered language to be complete as well. A key a language still
 * omits resolves through its `language.fallback` list, then English.
 */
import enUS from './en-US.json' with { type: 'json' };
import zhCN from './zh-CN.json' with { type: 'json' };
import zhTW from './zh-TW.json' with { type: 'json' };

export type Messages = typeof enUS;
export type Namespace = keyof Messages;
type LocaleFile = { [N in Namespace]?: { [K in keyof Messages[N]]?: string } } & {
  language?: { fallback?: readonly string[] };
};

// Every src/locales/<lang>.json is registered here, in language-picker order;
// the language type and list follow from this table.
const FILES = { 'zh-CN': zhCN, 'zh-TW': zhTW, 'en-US': enUS } satisfies Record<string, LocaleFile>;

export type Lang = keyof typeof FILES;
export const LANGS = Object.keys(FILES) as Lang[];
export const SOURCE_LANG: Lang = 'en-US';

export function isLang(value: unknown): value is Lang {
  return typeof value === 'string' && Object.hasOwn(FILES, value);
}

/** `lang`, its declared fallbacks (not followed further), then English. */
const CHAINS = Object.fromEntries(LANGS.map(lang => {
  const declared = (FILES[lang] as LocaleFile).language?.fallback ?? [];
  return [lang, [...new Set([lang, ...declared.filter(isLang), SOURCE_LANG])]];
})) as Record<Lang, Lang[]>;

export function fallbackChain(lang: Lang): readonly Lang[] {
  return CHAINS[lang];
}

// Null-prototype tables so data-derived keys never resolve to Object members.
const MERGED = Object.fromEntries(LANGS.map(lang => [lang, Object.fromEntries(
  (Object.keys(enUS) as Namespace[]).map(ns => [ns, Object.assign(
    Object.create(null),
    ...[...CHAINS[lang]].reverse().map(source => (FILES[source] as LocaleFile)[ns]),
  )]),
)])) as Record<Lang, Messages>;

/** One namespace in `lang`, resolving omitted keys through the fallback chain. */
export function messages<N extends Namespace>(lang: Lang, ns: N): Messages[N] {
  return MERGED[lang][ns];
}

/** Look up a data-derived key; undefined when no language in the chain defines it. */
export function lookup(lang: Lang, ns: Namespace, key: string): string | undefined {
  return (MERGED[lang][ns] as Record<string, string | undefined>)[key];
}

/** Like `lookup`, but only a non-English translation; English yields undefined. */
export function translated(lang: Lang, ns: Namespace, key: string): string | undefined {
  for (const source of CHAINS[lang]) {
    if (source === SOURCE_LANG) return undefined;
    const table = (FILES[source] as LocaleFile)[ns] as Record<string, string> | undefined;
    if (table && Object.hasOwn(table, key)) return table[key];
  }
  return undefined;
}
