import { describe, expect, it } from 'vitest';
import type { CraftingMetadata } from '../api/itemCrafting';
import { baseDraftText, craftingCatalog } from './itemCrafting';
import { availableAffixes, createAffixDraft, editableAffixPool } from './marketAffixEditor';
import type { TradeCatalog } from './tradeOptimizer';

const game: CraftingMetadata = {
  bases: [{ id: 'ring', name: 'Ring', item_class: 'Ring', tags: ['ring'], mod_domain: 1 },
    { id: 'excluded', name: 'Excluded Ring', item_class: 'Ring', tags: ['ring', 'excluded'], mod_domain: 1 }],
  mods: [
    { id: 'life', domain: 1, kind: 'prefix', level: 1, group: 'Life', spawn_weights: [{ tag: 'excluded', weight: 0 }, { tag: 'ring', weight: 100 }] },
    { id: 'high', domain: 1, kind: 'prefix', level: 80, group: 'Life', spawn_weights: [{ tag: 'default', weight: 1 }] },
    { id: 'mana', domain: 1, kind: 'prefix', level: 1, group: 'Mana', spawn_weights: [{ tag: 'ring', weight: 1 }] },
    { id: 'str', domain: 1, kind: 'suffix', level: 1, group: 'Strength', spawn_weights: [{ tag: 'ring', weight: 1 }] },
    { id: 'wrong', domain: 11, kind: 'prefix', level: 1, group: 'Wrong', spawn_weights: [{ tag: 'ring', weight: 1 }] },
    { id: 'missing', domain: 1, kind: 'suffix', level: 1, group: 'Missing', spawn_weights: [{ tag: 'ring', weight: 1 }] },
  ],
};
const display: TradeCatalog = {
  bases: ['Ring', 'Excluded Ring', 'Overlay only'].map(name => ({ name, category: 'accessory.ring', level: 12, tags: ['wrong'], implicits: ['+30% to Cold Resistance'], domain: 'equipment', affix_limit: 3 })),
  mods: game.mods.filter(mod => mod.id !== 'missing').map(mod => ({ id: `equipment:${mod.id}`, group: 'wrong', kind: 'suffix', level: 99,
    lines: ['+10 to maximum Life'], roll_lines: ['+(5-10) to maximum Life'], weights: [['default', 0]], stats: [], domain: 'equipment' })),
};

describe('game-backed item crafting', () => {
  const catalog = craftingCatalog(game, display);
  const base = catalog.bases.find(base => base.id === 'ring')!;
  const pool = editableAffixPool(catalog, base);
  it('uses game bases, domains and ordered weights, including zero exclusions and default', () => {
    expect(catalog.bases).toHaveLength(2);
    expect(catalog.missingMods).toBe(1);
    expect(pool.map(mod => mod.id)).toEqual(['life', 'high', 'mana', 'str']);
    expect(editableAffixPool(catalog, catalog.bases.find(base => base.id === 'excluded')!).map(mod => mod.id)).not.toContain('life');
    expect(pool[0]).toMatchObject({ group: 'Life', kind: 'prefix', level: 1 });
    const duplicates = craftingCatalog({ ...game, bases: [...game.bases, { ...game.bases[0], id: 'canonical' }] }, display);
    expect(duplicates.bases.filter(base => base.name === 'Ring').map(base => base.id)).toEqual(['canonical']);
  });
  it('restricts item level, exclusive groups and the magic prefix/suffix counts', () => {
    const draft = createAffixDraft(baseDraftText(base, 'MAGIC', 50, 0), base, [], pool);
    expect(availableAffixes(draft, pool, 100).map(mod => mod.id)).toEqual(['life', 'mana', 'str']);
    draft.rows.push({ key: 'one', modId: 'life', indices: [], original: [], roll: .5, edited: true });
    expect(availableAffixes(draft, pool, 100).map(mod => mod.id)).toEqual(['str']);
    draft.rows.push({ key: 'two', modId: 'str', indices: [], original: [], roll: .5, edited: true });
    expect(availableAffixes(draft, pool, 100)).toEqual([]);
    const rare = { ...draft, limit: 3, itemLevel: 100 };
    expect(availableAffixes(rare, pool, 100).map(mod => mod.id)).toEqual(['mana']);
    rare.rows.push({ key: 'three', modId: 'mana', indices: [], original: [], roll: .5, edited: true });
    // Prefix capacity counts actual mods, not rendered lines.
    const extra = { ...pool[2], id: 'extra', group: 'Extra' };
    rare.rows.push({ key: 'four', modId: 'extra', indices: [], original: [], roll: .5, edited: true });
    expect(availableAffixes(rare, [...pool, extra, { ...extra, id: 'fourth', group: 'Fourth' }], 100)).toEqual([]);
  });
  it('starts normal items without explicit affixes and retains base requirements/implicits', () => {
    const text = baseDraftText(base, 'NORMAL', 85, 20);
    expect(text).toBe('Rarity: NORMAL\nRing\nItem Level: 85\nQuality: 20\nLevelReq: 12\nImplicits: 1\n+30% to Cold Resistance');
    expect(createAffixDraft(text, base, [], pool).locked).toBe('rarity');
    expect(() => baseDraftText({ ...base, affix_limit: 1 }, 'RARE', 50, 0)).toThrow('invalid-rarity');
    expect(() => baseDraftText(base, 'RARE', 101, 0)).toThrow('invalid-draft');
    expect(() => baseDraftText(base, 'RARE', 85, -1)).toThrow('invalid-draft');
  });
});
