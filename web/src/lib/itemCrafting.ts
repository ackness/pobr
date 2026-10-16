import type { CraftingMetadata } from '../api/itemCrafting';
import type { TradeBase, TradeCatalog } from './tradeOptimizer';

export interface CraftBase extends TradeBase { id: string; itemClass: string }
export interface CraftCatalog extends TradeCatalog { bases: CraftBase[]; missingMods: number }

/** Game IDs/tags/weights determine eligibility; the shared display catalogue supplies text. */
export function craftingCatalog(game: CraftingMetadata, display: TradeCatalog): CraftCatalog {
  const basesByName = new Map(display.bases.map(base => [base.name, base]));
  // Text imports resolve colliding names to the last game record in BuildData too.
  const canonicalBases = [...new Map(game.bases.map(base => [base.name, base])).values()];
  const bases = canonicalBases.flatMap(base => {
    const shown = basesByName.get(base.name);
    return shown ? [{ ...shown, id: base.id, itemClass: base.item_class,
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
