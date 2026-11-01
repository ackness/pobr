import type { Lang } from './i18n';

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
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function nativeDisplayTexts(value: unknown, version: string, locale: Lang): NativeDisplayTexts {
  if (!object(value) || value.schema !== 'pobr-native-display-texts/v1' || value.game_patch !== version
    || value.locale !== locale || !['zh-CN', 'zh-TW'].includes(locale)
    || !['international', 'china'].includes(String(value.realm))
    || typeof value.catalog_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(value.catalog_sha256)
    || !object(value.entries) || !Object.values(value.entries).every(text => text === null || typeof text === 'string')
    || !object(value.scopes) || !Object.values(value.scopes).every(scope => object(scope)
      && Object.values(scope).every(pair => Array.isArray(pair) && pair.length === 2
        && pair.every(text => text === null || typeof text === 'string')))) {
    throw new Error('Invalid native display text identity or shape');
  }
  return value as unknown as NativeDisplayTexts;
}
