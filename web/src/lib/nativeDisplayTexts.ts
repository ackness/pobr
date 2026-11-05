import type { Lang } from './i18n';

/** Who wrote a shown text that is not official: the translation library's own supplement
 * (`agent` = drafted by a model and reviewed, `human` = written by a person). Official text has
 * no origin (`null`). */
export type TextOrigin = 'agent' | 'human';

/** A shown text and who wrote it when it is not official. */
export interface DisplayText { text: string; origin: TextOrigin | null }

/** Which texts of a catalog are the library's own supplement. Absent, or a text it does not
 * list, means official. Read values through {@link listedOrigin}. */
export interface NativeDisplayOrigins {
  /** Literal English text -> origin of its `entries` translation. */
  entries?: Record<string, unknown>;
  /** `Table.Field` -> id -> origin of that scope record's translation. */
  scopes?: Record<string, Record<string, unknown>>;
}

/** A display-only projection of the translation library's source-bound catalog.
 * The full provenance and unresolved-record reasons stay in the producer receipt. */
export interface NativeDisplayTexts {
  schema: 'pobr-native-display-texts/v1';
  game_patch: string;
  realm: 'international' | 'china';
  locale: 'zh-CN' | 'zh-TW';
  catalog_sha256: string;
  entries: Record<string, string | null>;
  scopes: Record<string, Record<string, [string | null, string | null]>>;
  origins?: NativeDisplayOrigins;
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** An origin map: text or id -> origin. Values are not checked here: see {@link listedOrigin}. */
const originMap = (value: unknown) => object(value);

/** The catalog's origin record for one text (`null` = official). `official` or `null` is the same
 * as not listing the text; any other value the web does not know reads as `agent`, as in
 * `stat_lines.json`, so a new or misspelt origin never passes supplement text off as official nor
 * drops the catalog. */
export const listedOrigin = (origins: Record<string, unknown> | undefined, key: string): TextOrigin | null =>
  !origins || !Object.hasOwn(origins, key) || origins[key] === 'official' || origins[key] === null ? null
    : origins[key] === 'human' ? 'human' : 'agent';

export function nativeDisplayTexts(value: unknown, version: string, locale: Lang): NativeDisplayTexts {
  if (!object(value) || value.schema !== 'pobr-native-display-texts/v1' || value.game_patch !== version
    || value.locale !== locale || !['zh-CN', 'zh-TW'].includes(locale)
    || !['international', 'china'].includes(String(value.realm))
    || typeof value.catalog_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(value.catalog_sha256)
    || !object(value.entries) || !Object.values(value.entries).every(text => text === null || typeof text === 'string')
    || !object(value.scopes) || !Object.values(value.scopes).every(scope => object(scope)
      && Object.values(scope).every(pair => Array.isArray(pair) && pair.length === 2
        && pair.every(text => text === null || typeof text === 'string')))
    // Optional and additive: absent (every catalog before supplements) means all official.
    || value.origins !== undefined && !(object(value.origins)
      && (value.origins.entries === undefined || originMap(value.origins.entries))
      && (value.origins.scopes === undefined || object(value.origins.scopes)
        && Object.values(value.origins.scopes).every(originMap)))) {
    throw new Error('Invalid native display text identity or shape');
  }
  return value as unknown as NativeDisplayTexts;
}
