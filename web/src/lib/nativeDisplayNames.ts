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
      // within Characters.Name; rejected duplicate candidates also block the
      // literal map, so they cannot disappear through a null scope English.
      if (characterScope) {
        const matches = Object.values(characterScope).filter(([english]) => english === character.name);
        if (matches.length || catalog.entries[character.name] === null) {
          classes[character.name] = matches.length === 1 && catalog.entries[character.name] !== null
            ? matches[0][1] : null;
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
