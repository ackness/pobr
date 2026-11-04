/**
 * Interface copy, one JSON file per language. Game names and modifier text
 * come from the versioned localization data instead.
 *
 * en-US.json is the complete key set. Other languages may omit a key, which
 * then renders in English; `mise run i18n:check` reports such gaps and rejects
 * unknown keys, mismatched placeholders and unregistered files.
 */
import enUS from './en-US.json' with { type: 'json' };
import zhCN from './zh-CN.json' with { type: 'json' };
import zhTW from './zh-TW.json' with { type: 'json' };

export type Messages = typeof enUS;
export type Namespace = keyof Messages;
type LocaleFile = { [N in Namespace]?: { [K in keyof Messages[N]]?: string } };

// Every src/locales/<lang>.json is registered here, in language-picker order;
// the language type and list follow from this table.
const FILES = { 'zh-CN': zhCN, 'zh-TW': zhTW, 'en-US': enUS } satisfies Record<string, LocaleFile>;

export type Lang = keyof typeof FILES;
export const LANGS = Object.keys(FILES) as Lang[];
export const SOURCE_LANG: Lang = 'en-US';

export function isLang(value: unknown): value is Lang {
  return typeof value === 'string' && Object.hasOwn(FILES, value);
}

// Null-prototype tables so data-derived keys never resolve to Object members.
const MERGED = Object.fromEntries(LANGS.map(lang => {
  const file: LocaleFile = FILES[lang];
  return [lang, Object.fromEntries((Object.keys(enUS) as Namespace[]).map(ns => [
    ns, Object.assign(Object.create(null), enUS[ns], file[ns]),
  ]))];
})) as Record<Lang, Messages>;

/** One namespace in `lang`, with English for keys the language omits. */
export function messages<N extends Namespace>(lang: Lang, ns: N): Messages[N] {
  return MERGED[lang][ns];
}

/** Look up a data-derived key; undefined when no language defines it. */
export function lookup(lang: Lang, ns: Namespace, key: string): string | undefined {
  return (MERGED[lang][ns] as Record<string, string | undefined>)[key];
}
