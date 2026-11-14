import type { CraftingMetadata } from '../api/itemCrafting';
import { basesForSlot, type TradeBase, type TradeCatalog } from './tradeOptimizer';
import { bindT, slotLabel, type Lang } from './i18n';

export interface CraftBase extends TradeBase { id: string; itemClass: string; displayNames?: CraftingMetadata['bases'][number]['display_names'] }
export interface CraftCatalog extends TradeCatalog { bases: CraftBase[]; missingMods: number }
export function craftBaseDisplayName(base: CraftBase, lang: Lang, fallback: string): string {
  return (lang === 'en-US' ? undefined : base.displayNames?.[lang]) ?? fallback;
}

const fixedItemClassLabel = (name: string, lang: Lang) =>
  name.replace(/\s/g, '') === 'BodyArmour' ? slotLabel(lang, 'bodyarmour') : undefined;

/** Whole-name aliases select old language-pack rows; an existing UI term stays the display fallback. */
export const itemClassName = (name: string, lang?: Lang) => (lang ? fixedItemClassLabel(name, lang) : undefined) ?? ({
  Amulet: 'Amulets', Belt: 'Belts', BodyArmour: 'Body Armours',
  Boots: 'Boots', Gloves: 'Gloves', Helmet: 'Helmets', Ring: 'Rings',
  LifeFlask: 'Life Flasks', ManaFlask: 'Mana Flasks', Charm: 'Charms',
  OneHandSword: 'One Hand Swords', TwoHandSword: 'Two Hand Swords', OneHandAxe: 'One Hand Axes',
  TwoHandAxe: 'Two Hand Axes', OneHandMace: 'One Hand Maces', TwoHandMace: 'Two Hand Maces',
  Bow: 'Bows', Crossbow: 'Crossbows', Staff: 'Staves', Wand: 'Wands', Sceptre: 'Sceptres',
  Shield: 'Shields', Quiver: 'Quivers', Spear: 'Spears', Quarterstaff: 'Quarterstaves',
} as Record<string, string>)[name.replace(/\s/g, '')] ?? name;

export function itemNameLabel(original: string, translated: string | undefined, lang: Lang): string {
  if (lang === 'en-US' || (translated === original && /\p{Script=Han}/u.test(original) && !/[A-Za-z]/.test(original))) return original;
  return translated && translated !== original && !/[A-Za-z]{2,}/.test(translated)
    ? translated : `${original}（${bindT(lang)('items.originalName')}）`;
}

export function itemClassLabel(original: string, translated: string | undefined, lang: Lang): string {
  const shown = itemNameLabel(original, translated, lang);
  if (lang !== 'en-US' && translated && shown === translated) return shown;
  const fixed = fixedItemClassLabel(original, lang);
  if (fixed) return fixed;
  return lang === 'en-US' ? original.replace(/([a-z])([A-Z])/g, '$1 $2') : shown;
}

export function defaultCraftClass(catalog: CraftCatalog, slot: string | null | undefined, text?: string): string {
  const header = text?.split('\n').map(line => line.trim()).filter(Boolean);
  const name = header?.[/^Rarity: (NORMAL|MAGIC)$/i.test(header[0]) ? 1 : 2];
  const equipped = name && [...catalog.bases].sort((a, b) => b.name.length - a.name.length).find(base =>
    name === base.name || (/^Rarity: MAGIC$/i.test(header![0]) && (name.endsWith(` ${base.name}`) || name.startsWith(`${base.name} of `) || name.includes(` ${base.name} of `))));
  if (equipped) return equipped.itemClass;
  const allowed = slot ? basesForSlot(catalog, slot) : [];
  return catalog.bases.find(base => allowed.some(candidate => candidate.name === base.name))?.itemClass
    ?? (catalog.bases.some(base => base.itemClass === 'Ring') ? 'Ring' : catalog.bases[0]?.itemClass ?? '');
}

/** Game IDs/tags/weights determine eligibility; the shared display catalogue supplies text. */
export function craftingCatalog(game: CraftingMetadata, display: TradeCatalog): CraftCatalog {
  const basesByName = new Map(display.bases.map(base => [base.name, base]));
  // Text imports resolve colliding names to the last game record in BuildData too.
  const canonicalBases = [...new Map(game.bases.map(base => [base.name, base])).values()];
  const bases = canonicalBases.flatMap(base => {
    const shown = basesByName.get(base.name);
    return shown ? [{ ...shown, id: base.id, itemClass: base.item_class, displayNames: base.display_names,
      tags: [...new Set(['default', ...base.tags])], domain: String(base.mod_domain) }] : [];
  }).sort((a, b) => a.itemClass.localeCompare(b.itemClass) || a.level - b.level || a.name.localeCompare(b.name));
  const textById = new Map(display.mods.map(mod => [mod.id.slice(mod.id.indexOf(':') + 1), mod]));
  let missingMods = 0;
  const mods = game.mods.flatMap(mod => {
    if (!bases.some(base => base.domain === String(mod.domain)
      && (mod.spawn_weights.find(({ tag }) => base.tags.includes(tag))?.weight ?? 0) > 0)) return [];
    const shown = textById.get(mod.id);
    if (!shown?.roll_lines || shown.roll_lines.length !== shown.lines.length) { missingMods++; return []; }
    return [{ ...shown, id: mod.id, group: mod.group, kind: mod.kind, level: mod.level,
      domain: String(mod.domain), weights: mod.spawn_weights.map(({ tag, weight }) => [tag, weight] as [string, number]) }];
  });
  return { bases, mods, missingMods };
}

/** Seed only. The backend ItemDraft writer canonicalizes this before any storage or comparison. */
export function baseDraftText(base: TradeBase, rarity: 'NORMAL' | 'MAGIC' | 'RARE', itemLevel: number, quality: number): string {
  if (rarity === 'RARE' && base.affix_limit === 1) throw new Error('invalid-rarity');
  if (!Number.isInteger(itemLevel) || itemLevel < 1 || itemLevel > 100
    || !Number.isInteger(quality) || quality < 0 || quality > 20) throw new Error('invalid-draft');
  return [`Rarity: ${rarity}`, ...(rarity === 'RARE' ? [base.name] : []), base.name,
    `Item Level: ${itemLevel}`, `Quality: ${quality}`, `LevelReq: ${base.level}`,
    ...(base.radius ? [`Radius: ${base.radius}`] : []),
    `Implicits: ${base.implicits.length}`, ...base.implicits].join('\n');
}
