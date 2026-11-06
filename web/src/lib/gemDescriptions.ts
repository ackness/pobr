import type { GemCatalogEntry } from '../api/types';
import { chineseScript } from './chinese';
import { gameTextChain } from './gameLocales';
import type { Lang } from './i18n';
import type { GemNameLink, GemNameOverride } from './nativeDisplayNames';
import type { NativeDisplayTexts } from './nativeDisplayTexts';

const ACTIVE = 'ActiveSkills.Description';
const SUPPORT = 'GemEffects.SupportText';
type Scope = typeof ACTIVE | typeof SUPPORT;
type DescriptionScopes = Record<Scope, Record<string, string>>;
export type GemAttribute = 'str' | 'dex' | 'int';
export interface GemDescription { text: string; lang: Lang | 'en'; english: boolean }
export interface GemDescriptionLink extends GemNameOverride { variant_id: string }
export interface GemRequirementLink extends GemNameLink { str_pct?: number; dex_pct?: number; int_pct?: number }

/** Plain data, so the same index can cross the engine Worker's structured-clone boundary. */
export interface GemDescriptionIndex {
  scopes: DescriptionScopes;
  variants: Record<string, string[]>;
  activeSkills: Record<string, string | null>;
  requirements: Record<string, GemAttribute[] | null>;
  catalogs: Array<Pick<NativeDisplayTexts, 'locale' | 'scopes'>>;
}

/** Game markup is rendered only at the display boundary; raw English remains source identity.
 * Only `[A]` and `[A|B]` exist: any other form (`[A|B|C]`) keeps its brackets, so the caller rejects it. */
export function renderGemDescription(raw: string): string {
  return raw.replace(/\[([^\[\]|]*)(?:\|([^\[\]|]*))?\]/g, (_match, a: string, b: string | undefined) => b ?? a)
    .replace(/\r\n|\\r\\n|\\n/g, '\n').trim();
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function gemDescriptionScopes(value: unknown): DescriptionScopes {
  const scopes = object(value) && object(value.scopes) ? value.scopes : null;
  if (!scopes || Object.keys(scopes).length !== 2 || ![ACTIVE, SUPPORT].every(key => {
    const scope = scopes[key];
    return Object.hasOwn(scopes, key) && object(scope)
      && Object.values(scope).every(text => typeof text === 'string' && text.trim().length > 0);
  })) {
    throw new Error('Invalid gem description shape');
  }
  return scopes as DescriptionScopes;
}

export function buildGemDescriptionIndex(
  descriptions: unknown, links: readonly GemDescriptionLink[], activeSkills: GemDescriptionIndex['activeSkills'],
  catalogs: readonly NativeDisplayTexts[], gems: readonly GemRequirementLink[],
): GemDescriptionIndex {
  const variants: GemDescriptionIndex['variants'] = Object.create(null);
  for (const link of links) {
    const ids = variants[link.granted_effect_id] ??= [];
    if (!ids.includes(link.variant_id)) ids.push(link.variant_id);
  }
  const overrides = new Map(links.map(link => [link.gem_id, link.granted_effect_id]));
  const requirements: GemDescriptionIndex['requirements'] = Object.create(null);
  for (const gem of gems) {
    const effect = overrides.get(gem.id) ?? gem.granted_effect_id;
    if (!effect) continue;
    const attrs = (['str', 'dex', 'int'] as const).filter(attr => (gem[`${attr}_pct`] ?? 0) > 0);
    if (!Object.hasOwn(requirements, effect)) requirements[effect] = attrs;
    else if (JSON.stringify(requirements[effect]) !== JSON.stringify(attrs)) requirements[effect] = null;
  }
  return { scopes: gemDescriptionScopes(descriptions), variants, activeSkills, requirements,
    catalogs: catalogs.map(({ locale, scopes }) => ({ locale, scopes: {
      [ACTIVE]: scopes[ACTIVE] ?? {}, [SUPPORT]: scopes[SUPPORT] ?? {},
    } })) };
}

/** Optional pack member: no dependent files are read when the inventory lacks descriptions. */
export async function loadGemDescriptionIndex(
  files: readonly string[], readJson: (path: string) => Promise<unknown>,
  catalogs: () => Promise<readonly NativeDisplayTexts[]>,
): Promise<GemDescriptionIndex | null> {
  if (!files.includes('overlay/gem_descriptions.json')) return null;
  const optional = (path: string, fallback: unknown) => files.includes(path) ? readJson(path) : Promise.resolve(fallback);
  const [descriptions, effects, activeSkills, gems, display] = await Promise.all([
    readJson('overlay/gem_descriptions.json'), optional('overlay/gem_effects.json', { gems: [] }),
    optional('base/granted_effects.json', []).then(rows => Object.fromEntries(
      (rows as Array<{ id: string; active_skill?: string | null }>).map(row => [row.id, row.active_skill ?? null]))),
    optional('base/skill_gems.json', []), catalogs(),
  ]);
  return buildGemDescriptionIndex(descriptions, (effects as { gems: GemDescriptionLink[] }).gems,
    activeSkills as GemDescriptionIndex['activeSkills'], display, gems as GemRequirementLink[]);
}

function resolveMember(index: GemDescriptionIndex, scope: Scope, id: string, raw: string, lang: Lang): GemDescription | null {
  const text = renderGemDescription(raw);
  if (!text || /[\[\]]/.test(text)) return null;
  const english: GemDescription = { text, lang: 'en', english: true };
  for (const code of gameTextChain(lang)) {
    const record = index.catalogs.find(catalog => catalog.locale === code)?.scopes[scope]?.[id];
    if (!record || record[0] !== raw) continue;
    if (!record[1]?.trim()) return english;
    const translated = renderGemDescription(record[1]);
    if (/[\[\]]/.test(translated)) continue;
    if (!translated) return english;
    const shown = code === lang ? translated : chineseScript(translated, lang);
    return shown === text ? english : { text: shown, lang, english: false };
  }
  return english;
}

/** Resolve every English-bearing variant independently, then require consensus. Never pick by order. */
export function gemDescription(index: GemDescriptionIndex, gem: Pick<GemCatalogEntry, 'skill_id' | 'is_support'>, lang: Lang): GemDescription | null {
  const scope = gem.is_support ? SUPPORT : ACTIVE;
  const active = index.activeSkills[gem.skill_id];
  const ids = gem.is_support ? index.variants[gem.skill_id] ?? [] : active ? [active] : [];
  const members = ids.filter(id => Object.hasOwn(index.scopes[scope], id));
  if (!members.length) return null;
  const raw = index.scopes[scope][members[0]];
  if (!members.every(id => index.scopes[scope][id] === raw)) return null;
  const resolved = members.map(id => resolveMember(index, scope, id, raw, lang));
  const first = resolved[0];
  if (!first) return null;
  return resolved.every(value => value?.text === first.text && value?.lang === first.lang && value?.english === first.english)
    ? first : resolveMember(index, scope, members[0], raw, 'en-US');
}
