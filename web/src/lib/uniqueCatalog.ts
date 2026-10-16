import type { UniqueEntry, UniqueSelection } from '../api/uniques';
import type { CraftingMetadata } from '../api/itemCrafting';
import type { TradeBase, TradeCatalog } from './tradeOptimizer';
import { basesForSlot } from './tradeOptimizer';

const fields = (entry: UniqueEntry, key: string) => entry.raw.split('\n').filter(line => line.startsWith(`${key}:`)).map(line => line.slice(key.length + 1).trim());
const ids = (line: string, key: string): number[] | undefined => {
  const value = line.match(new RegExp(`\\{${key}:([^}]+)\\}`))?.[1];
  return value?.split(',').map(Number);
};
export function uniqueOptions(entry: UniqueEntry) {
  return { variants: fields(entry, 'Variant'), versions: fields(entry, 'Version') };
}
export function uniqueGroups(entry: UniqueEntry, version?: number): { id: number; choices: number[] }[] {
  const groups = new Map<number, Set<number>>();
  for (const line of entry.raw.split('\n')) {
    if (ids(line, 'version') && !ids(line, 'version')!.includes(version ?? 1)) continue;
    for (const id of ids(line, 'group') ?? []) {
      const choices = groups.get(id) ?? new Set<number>();
      for (const variant of ids(line, 'variant') ?? []) choices.add(variant);
      groups.set(id, choices);
    }
  }
  return [...groups].sort(([a], [b]) => a - b).map(([id, choices]) => ({ id, choices: [...choices].sort((a, b) => a - b) }));
}
export function defaultUniqueSelection(entry: UniqueEntry): UniqueSelection {
  const { variants, versions } = uniqueOptions(entry);
  const defaultIndex = (names: string[], key: string) => names.length
    ? Number(fields(entry, key)[0]) || (names.lastIndexOf('Current') >= 0 ? names.lastIndexOf('Current') + 1 : names.length) : undefined;
  const version = defaultIndex(versions, 'Selected Version');
  const alts = ['', ' Two', ' Three', ' Four', ' Five'].flatMap(suffix => fields(entry, `Has Alt Variant${suffix}`)[0] === 'true'
    ? [defaultIndex(variants, `Selected Alt Variant${suffix}`) ?? 1] : []);
  const used = new Set<number>();
  const groups: Record<number, number> = {};
  for (const group of uniqueGroups(entry, version)) {
    const choice = group.choices.find(value => !used.has(value))!;
    groups[group.id] = choice; used.add(choice);
  }
  return { id: entry.id, variant: defaultIndex(variants, 'Selected Variant'), version, alts, groups, rolls: {} };
}

/** Visibility only; the backend's existing item parser makes the actual selection and resolves values. */
export function uniqueRolls(entry: UniqueEntry, selection: UniqueSelection) {
  const grouped = uniqueGroups(entry, selection.version).length > 0;
  const chosen = [selection.variant, ...selection.alts];
  return entry.raw.split('\n').flatMap((line, index) => {
    const variants = ids(line, 'variant'), versions = ids(line, 'version'), groups = ids(line, 'group');
    if (versions && !versions.includes(selection.version ?? 1)) return [];
    if (groups && !groups.some(group => variants?.includes(selection.groups[group]))) return [];
    if (variants && !groups && (grouped || !chosen.some(value => value !== undefined && variants.includes(value)))) return [];
    return /\(-?\d+(?:\.\d+)?--?\d+(?:\.\d+)?\)/.test(line)
      ? [{ index, text: line.replace(/\{[^}]*\}/g, ''), roll: selection.rolls[index] ?? .5 }] : [];
  });
}

export function filterUniques(entries: UniqueEntry[], query: string, itemClass: string, slot: string,
  catalog: TradeCatalog, localized: ReadonlyMap<number, string> = new Map()): UniqueEntry[] {
  const needle = query.trim().toLocaleLowerCase();
  const allowed = slot ? new Set(basesForSlot(catalog, slot).map(base => base.name)) : null;
  return entries.filter(entry => (!itemClass || entry.bases.some(base => base.item_class === itemClass))
    && (!allowed || entry.bases.some(base => allowed.has(base.name)))
    && (!needle || `${entry.name}\n${entry.raw}\n${localized.get(entry.id) ?? ''}`.toLocaleLowerCase().includes(needle)));
}

/** Unique-only bases do not belong in trade searches. Add them only to this local preview catalogue. */
export function uniquePreviewCatalog(entries: UniqueEntry[], catalog: TradeCatalog, game: CraftingMetadata): TradeCatalog {
  const known = new Map(catalog.bases.map(base => [base.name, base]));
  const categories = new Map<string, TradeBase>();
  for (const base of game.bases) {
    const template = known.get(base.name);
    if (template && !categories.has(base.item_class)) categories.set(base.item_class, template);
  }
  for (const entry of entries) for (const base of entry.bases) {
    if (known.has(base.name)) continue;
    const template = categories.get(base.item_class);
    if (template) known.set(base.name, { ...template, name: base.name, tags: [], implicits: [],
      level: 1, requirement_known: false });
  }
  return { ...catalog, bases: [...known.values()] };
}
