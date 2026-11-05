import type { ClassNames, GemCatalogEntry, PassiveTreeMeta } from '../api/types';
import { localizedName } from './chinese';
import type { Lang } from './i18n';
import type { NativeDisplayTexts } from './nativeDisplayTexts';
import type { CraftingMetadata } from '../api/itemCrafting';

/** The same gem -> effect links consumed by GameData, including its overlay. */
export interface GemNameLink { id: string; granted_effect_id?: string | null }
export interface GemNameOverride { gem_id: string; granted_effect_id: string }

type Scope = NativeDisplayTexts['scopes'][string];
const own = (scope: Scope, id: string) => Object.hasOwn(scope, id);

/** Only the exact source-owned base and its current English can supply a name. */
export function nativeCraftBaseNames(data: CraftingMetadata, catalogs: readonly NativeDisplayTexts[]): CraftingMetadata {
  return { ...data, bases: data.bases.map(base => {
    const displayNames = { ...base.display_names };
    for (const catalog of catalogs) {
      const scope = catalog.scopes['BaseItemTypes.Name'];
      if (!scope || !own(scope, base.id)) continue;
      const [english, translated] = scope[base.id];
      displayNames[catalog.locale] = english === base.name && translated?.trim() ? translated : base.name;
    }
    return Object.keys(displayNames).length ? { ...base, display_names: displayNames } : base;
  }) };
}

/** The WASM catalog selects a base but does not expose its id. Keep every linked
 * base consistent with its current English name; never guess a winner by order.
 * undefined means unowned, null means explicitly unresolved. */
function linkedName(scope: Scope, ids: readonly string[], english: string): string | null | undefined {
  if (!ids.some(id => own(scope, id))) return undefined;
  const possible = ids.filter(id => !own(scope, id) || scope[id][0] === null || scope[id][0] === english);
  if (!possible.length || possible.some(id => !own(scope, id) || scope[id][0] !== english || !scope[id][1]?.trim())) return null;
  const translations = new Set(possible.map(id => scope[id][1]!));
  return translations.size === 1 ? [...translations][0] : null;
}

/** A displayed name whose game identity is known from where it is shown, not guessed from its text. */
export type GameNameRequest =
  /** A passive node: its PassiveSkills Id (`passive_tree.json` `id`) and its current effective name. */
  | { kind: 'passive'; id: string; name: string }
  /** A line the item classifier or the item format marks as the base type. */
  | { kind: 'base'; name: string };

/** One id-scoped record checked against the English being shown. A different
 * English means the id now shows another text (a transformed node or stale
 * catalog): that record says nothing about it. */
function scopedName(scope: Scope | undefined, id: string, english: string): string | null | undefined {
  if (!scope || !own(scope, id)) return undefined;
  const [source, translated] = scope[id];
  if (source !== null && source !== english) return undefined;
  return source === english && translated?.trim() ? translated : null;
}

/** Id-scoped names from each locale's own catalog; never another locale's.
 * undefined means unowned (use the text path), null means explicitly unresolved (show English). */
export class NativeNameIndex {
  private readonly catalogs = new Map<Lang, NativeDisplayTexts>();
  private readonly baseIds = new Map<string, string[]>();

  constructor(catalogs: readonly NativeDisplayTexts[], bases: readonly { id: string; name: string }[] = []) {
    for (const catalog of catalogs) {
      if (this.catalogs.has(catalog.locale)) throw new Error('Duplicate native display locale');
      this.catalogs.set(catalog.locale, catalog);
    }
    for (const base of bases) {
      const ids = this.baseIds.get(base.name) ?? [];
      ids.push(base.id);
      this.baseIds.set(base.name, ids);
    }
  }

  resolve(request: GameNameRequest, lang: Lang): string | null | undefined {
    const catalog = this.catalogs.get(lang);
    if (!catalog || !request.name) return undefined;
    if (request.kind === 'passive') return scopedName(catalog.scopes['PassiveSkills.Name'], request.id, request.name);
    const scope = catalog.scopes['BaseItemTypes.Name'];
    return scope ? linkedName(scope, this.baseIds.get(request.name) ?? [], request.name) : undefined;
  }

  /** `translateNames` in two steps: `pending` are the English names the catalog does not own;
   * `merge` takes their text-path translations, in order, and returns every displayed name. */
  split(requests: readonly GameNameRequest[], lang: Lang): { pending: string[]; merge: (texts: readonly string[]) => string[] } {
    const scoped = requests.map(request => this.resolve(request, lang));
    const pending = requests.filter((_, index) => scoped[index] === undefined).map(request => request.name);
    return { pending, merge: texts => {
      let next = 0;
      return requests.map((request, index) => scoped[index] === undefined
        ? texts[next++] ?? request.name : scoped[index] ?? request.name);
    } };
  }
}

export function nativeGemNames(
  entries: readonly GemCatalogEntry[], gems: readonly GemNameLink[], overrides: readonly GemNameOverride[],
  catalogs: readonly NativeDisplayTexts[],
): GemCatalogEntry[] {
  const byGem = new Map(overrides.map(link => [link.gem_id, link.granted_effect_id]));
  const byEffect = new Map<string, Set<string>>();
  for (const gem of gems) {
    const effect = byGem.get(gem.id) ?? gem.granted_effect_id;
    if (!effect) continue;
    const ids = byEffect.get(effect) ?? new Set<string>();
    ids.add(gem.id);
    byEffect.set(effect, ids);
  }
  return entries.map(entry => {
    const result = { ...entry }, ids = [...(byEffect.get(entry.skill_id) ?? [])];
    for (const catalog of catalogs) {
      const scope = catalog.scopes['BaseItemTypes.Name'];
      if (!scope) continue;
      const name = linkedName(scope, ids, entry.name);
      if (name !== undefined) {
        // A nonempty English result also stops localizedName's alternate-script
        // fallback. The original WASM data and all calculation keys stay intact.
        result[catalog.locale === 'zh-CN' ? 'name_zh_cn' : 'name_zh_tw'] = name ?? entry.name;
      }
    }
    return result;
  });
}

export function nativeClassNames(
  legacy: ClassNames, meta: PassiveTreeMeta, catalogs: readonly NativeDisplayTexts[],
): ClassNames {
  const locales: NonNullable<ClassNames['locales']> = {};
  for (const catalog of catalogs) {
    const classes: Record<string, string | null> = Object.create(null);
    const ascendancies: Record<string, string | null> = Object.create(null);
    const characterScope = catalog.scopes['Characters.Name'];
    const ascendancyScope = catalog.scopes['Ascendancy.Name'];
    for (const character of meta.classes) {
      // PassiveClass has no Characters.Id. Use only its unique current English
      // within Characters.Name. A null literal entry can come from another table
      // using the same English (Words "Shadow" is 阴影, the class is 暗影), so it
      // blocks a unique scope match only when this scope also holds a record
      // without English, which may be a rejected duplicate candidate.
      if (characterScope) {
        const matches = Object.values(characterScope).filter(([english]) => english === character.name);
        const hidden = catalog.entries[character.name] === null
          && Object.values(characterScope).some(([english]) => english === null);
        if (matches.length || catalog.entries[character.name] === null) {
          classes[character.name] = matches.length === 1 && !hidden && matches[0][1]?.trim() ? matches[0][1] : null;
        }
      }
      for (const ascendancy of character.ascendancies ?? []) {
        if (!ascendancyScope || !own(ascendancyScope, ascendancy.id)) continue;
        const pair = ascendancyScope[ascendancy.id];
        const name = pair[0] === ascendancy.name ? pair[1] : null;
        ascendancies[ascendancy.name] = Object.hasOwn(ascendancies, ascendancy.name)
          && ascendancies[ascendancy.name] !== name ? null : name;
      }
    }
    locales[catalog.locale] = { classes, ascendancies };
  }
  return { ...legacy, locales };
}

/** Class labels share one display boundary across selectors, summaries and sets. */
export function classDisplayName(
  names: ClassNames, english: string, lang: Lang, kind?: 'classes' | 'ascendancies',
): string {
  if (lang === 'en-US') return english;
  const groups = kind ? [kind] : ['ascendancies', 'classes'] as const;
  for (const group of groups) {
    const map = names.locales?.[lang]?.[group];
    if (map && Object.hasOwn(map, english)) return map[english] ?? english;
  }
  const legacy = groups.map(group => names[group][english]).find(Boolean);
  return localizedName(lang, english, legacy);
}
