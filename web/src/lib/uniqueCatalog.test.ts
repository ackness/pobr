import { describe, expect, it } from 'vitest';
import type { UniqueEntry } from '../api/uniques';
import type { TradeCatalog } from './tradeOptimizer';
import { defaultUniqueSelection, filterUniques, uniqueGroups, uniqueOptions, uniquePreviewCatalog, uniqueRolls } from './uniqueCatalog';

const ring: UniqueEntry = { id: 0, name: 'Example Ring', item_type: 'ring', bases: [{ name: 'Iron Ring', item_class: 'Ring' }], raw:
  'Example Ring\nIron Ring\nVariant: Old\nVariant: Current\nImplicits: 0\n{variant:1}+(10-20) to maximum Life\n{variant:2}+(30-40) to maximum Life\n+(5-10)% to Cold Resistance' };
const amulet: UniqueEntry = { id: 1, name: 'Example Amulet', item_type: 'amulet', bases: [{ name: 'Amber Amulet', item_class: 'Amulet' }], raw:
  'Example Amulet\nAmber Amulet\nVersion: Old\nVersion: Current\n{version:1}+(5-10) to Dexterity\n{version:2}+(20-30) to Dexterity' };
const catalog: TradeCatalog = { mods: [], bases: [
  { name: 'Iron Ring', category: 'accessory.ring', tags: [], implicits: [], level: 1 },
  { name: 'Amber Amulet', category: 'accessory.amulet', tags: [], implicits: [], level: 1 },
] };

describe('unique catalogue', () => {
  it('searches names, effect prose and localized aliases with class/slot filters', () => {
    const entries = [ring, amulet];
    expect(filterUniques(entries, 'EXAMPLE RING', '', '', catalog)).toEqual([ring]);
    expect(filterUniques(entries, 'cold resistance', '', '', catalog)).toEqual([ring]);
    expect(filterUniques(entries, '', 'Amulet', '', catalog)).toEqual([amulet]);
    expect(filterUniques(entries, '', '', 'ring1', catalog)).toEqual([ring]);
    expect(filterUniques(entries, 'life', 'Amulet', 'amulet', catalog)).toEqual([]);
    expect(filterUniques(entries, '冰冷抗性', '', '', catalog, new Map([[0, '冰冷抗性']]))).toEqual([ring]);
  });
  it('defaults to Current and midpoint rolls and keeps raw line indices stable across variants', () => {
    expect(uniqueOptions(ring).variants).toEqual(['Old', 'Current']);
    const selection = defaultUniqueSelection(ring);
    expect(selection.variant).toBe(2);
    expect(uniqueRolls(ring, selection)).toEqual([
      { index: 6, text: '+(30-40) to maximum Life', roll: .5 }, { index: 7, text: '+(5-10)% to Cold Resistance', roll: .5 },
    ]);
    expect(uniqueRolls(ring, { ...selection, variant: 1, rolls: { 5: 1, 6: 0 } })[0]).toEqual({ index: 5, text: '+(10-20) to maximum Life', roll: 1 });
    expect(uniqueRolls(amulet, defaultUniqueSelection(amulet))[0].text).toBe('+(20-30) to Dexterity');
  });
  it('preserves explicit defaults and supports alternate and grouped variants', () => {
    const multi = { ...ring, raw: ring.raw + '\nHas Alt Variant: true\nSelected Variant: 1\nSelected Alt Variant: 2' };
    expect(defaultUniqueSelection(multi)).toMatchObject({ variant: 1, alts: [2] });
    expect(uniqueRolls(multi, defaultUniqueSelection(multi))).toHaveLength(3);
    const grouped = { ...ring, raw: ring.raw + '\n{group:1}{variant:1}+(1-2) to Strength\n{group:1}{variant:2}+(3-4) to Strength\n{group:2}{variant:2}+(5-6) to Dexterity' };
    expect(uniqueGroups(grouped)).toEqual([{ id: 1, choices: [1, 2] }, { id: 2, choices: [2] }]);
    expect(defaultUniqueSelection(grouped).groups).toEqual({ 1: 1, 2: 2 });
    expect(uniqueRolls(grouped, defaultUniqueSelection(grouped)).map(line => line.text)).toEqual([
      '+(5-10)% to Cold Resistance', '+(1-2) to Strength', '+(5-6) to Dexterity',
    ]);
  });
  it('adds unique-only bases using the game class without inventing requirement metadata', () => {
    const unique = { ...ring, bases: [{ name: 'Hidden Ring', item_class: 'Ring' }] };
    const expanded = uniquePreviewCatalog([unique], catalog, { mods: [], bases: [
      { id: 'ring', name: 'Iron Ring', item_class: 'Ring', tags: [], mod_domain: 1 },
    ] });
    expect(expanded.bases.at(-1)).toMatchObject({ name: 'Hidden Ring', category: 'accessory.ring', requirement_known: false });
    expect(filterUniques([unique], '', '', 'ring1', expanded)).toEqual([unique]);
    expect(catalog.bases).toHaveLength(2);
  });
});
