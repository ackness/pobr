import { expect, test } from 'vitest';
import type { ItemAugmentInfo, RuneCatalogEntry } from '../api/types';
import { applyEquippedAugmentLimits, planReplacementAugments, type AugmentUsage } from './replacementAugments';

const option = (name: string, required_level = 1): RuneCatalogEntry => ({
  name, required_level, kind: 'Rune', is_soul_core: false, name_zh_cn: null, name_zh_tw: null, lines: ['Synthetic effect'],
});
const info = (overrides: Partial<ItemAugmentInfo> = {}): ItemAugmentInfo => ({
  sockets: 0, max_sockets: 2, runes: [], editable: true,
  options: [option('Life Rune'), option('Damage Rune'), option('Advanced Rune', 80)], ...overrides,
});

test('an anonymous augment candidate remains viewable but its equipped limits are explicitly unverified', () => {
  const candidate = info({ sockets: 1, runes: [''], editable: false, reason: 'unknown_augments' });
  expect(planReplacementAugments(candidate, undefined, 80, { mode: 'original' }, { catalog: [], items: [], slot: 'bodyarmour' }))
    .toMatchObject({ source: 'original', limitWarning: 'unknown-equipped-augments' });
});

test('an empty candidate inherits fitting augments and the needed capacity without mutating source metadata', () => {
  const candidate = info();
  const current = info({ sockets: 2, runes: ['Life Rune', 'Damage Rune'] });
  const before = structuredClone({ candidate, current });
  const plan = planReplacementAugments(candidate, current, 72);
  expect(plan).toMatchObject({ source: 'inherited', sockets: 2, runes: ['Life Rune', 'Damage Rune'], addedSockets: 2, skipped: [] });
  expect({ candidate, current }).toEqual(before);
});

test('inheritance filters incompatible and too-high-level augments before applying the socket capacity', () => {
  const candidate = info({ sockets: 1, max_sockets: 2 });
  const current = info({ sockets: 5, runes: ['Other Item Type Rune', 'Life Rune', 'Advanced Rune', 'Damage Rune', 'Life Rune'] });
  const plan = planReplacementAugments(candidate, current, 72);
  expect(plan).toMatchObject({ source: 'inherited', sockets: 2, runes: ['Life Rune', 'Damage Rune'], addedSockets: 1 });
  expect(plan.skipped).toEqual(['Other Item Type Rune', 'Advanced Rune', 'Life Rune']);
});

test('an existing listing augment preserves the entire listing including its empty sockets', () => {
  const candidate = info({ sockets: 2, runes: ['', 'Life Rune'] });
  const current = info({ sockets: 2, runes: ['Damage Rune', 'Damage Rune'] });
  expect(planReplacementAugments(candidate, current, 72)).toMatchObject({ source: 'original', runes: ['', 'Life Rune'], sockets: 2, addedSockets: 0 });
});

test('as-listed and non-editable candidates preserve their original augments and capacity', () => {
  const candidate = info({ sockets: 1, runes: ['Life Rune'] });
  const current = info({ sockets: 2, runes: ['Damage Rune', 'Damage Rune'] });
  expect(planReplacementAugments(candidate, current, 72, { mode: 'original' })).toMatchObject({ source: 'original', sockets: 1, runes: ['Life Rune'] });
  expect(planReplacementAugments({ ...candidate, editable: false, reason: 'special-rule' }, current, 72,
    { mode: 'custom', sockets: 2, runes: ['Damage Rune'] })).toMatchObject({ source: 'original', sockets: 1, runes: ['Life Rune'] });
});

test('customization preserves explicit empty slots and permits removing all sockets', () => {
  const candidate = info({ sockets: 1, runes: ['Life Rune'] });
  expect(planReplacementAugments(candidate, undefined, 80, { mode: 'custom', sockets: 2, runes: ['', 'Advanced Rune'] }))
    .toMatchObject({ source: 'custom', sockets: 2, runes: ['', 'Advanced Rune'], addedSockets: 1 });
  expect(planReplacementAugments(candidate, undefined, 80, { mode: 'custom', sockets: 0, runes: [] }))
    .toMatchObject({ source: 'custom', sockets: 0, runes: [], addedSockets: 0 });
});

test('customization rejects invalid capacity, incompatible augments and unmet required levels', () => {
  const candidate = info();
  for (const selection of [
    { sockets: -1, runes: [] }, { sockets: 1.5, runes: [] }, { sockets: Number.NaN, runes: [] },
    { sockets: 3, runes: [] }, { sockets: 1, runes: ['Life Rune', 'Damage Rune'] },
    { sockets: 1, runes: ['Other Item Type Rune'] }, { sockets: 1, runes: ['Advanced Rune'] },
  ]) expect(() => planReplacementAugments(candidate, undefined, 72, { mode: 'custom', ...selection })).toThrow('invalid-augments');
});

test('no source sockets or augments leaves a bare candidate unchanged', () => {
  expect(planReplacementAugments(info(), undefined, 72).source).toBe('original');
  expect(planReplacementAugments(info({ sockets: 1 }), info(), 72)).toMatchObject({ source: 'original', sockets: 1, runes: [] });
});

test('limited augments are inherited only once and duplicate custom selections are rejected', () => {
  const candidate = info({ options: [{ ...option('Limited Rune'), limit: 1 }, option('Damage Rune')] });
  const current = info({ sockets: 2, runes: ['Limited Rune', 'Limited Rune', 'Damage Rune'] });
  expect(planReplacementAugments(candidate, current, 72)).toMatchObject({
    source: 'inherited', sockets: 2, runes: ['Limited Rune', 'Damage Rune'], skipped: ['Limited Rune'],
  });
  expect(() => planReplacementAugments(candidate, undefined, 72,
    { mode: 'custom', sockets: 2, runes: ['Limited Rune', 'Limited Rune'] })).toThrow('invalid-augments');
  expect(planReplacementAugments(candidate, undefined, 72,
    { mode: 'custom', sockets: 2, runes: ['Limited Rune', ''] }).runes).toEqual(['Limited Rune', '']);
});

const vitality: RuneCatalogEntry = { ...option('Rune of Vitality'), limit: 1,
  name_zh_cn: '活力符文', name_zh_tw: '活力符文繁' };
const serpent: RuneCatalogEntry = { ...option('Ancient Serpent Idol'), kind: 'Idol', limit: 1, limit_id: 'AncientAugment' };
const fox: RuneCatalogEntry = { ...option('Ancient Fox Idol'), kind: 'Idol', limit: 1, limit_id: 'AncientAugment' };
const globalCatalog = [vitality, serpent, fox, option('Damage Rune')];
const usageFor = (items: AugmentUsage['items'], slot = 'weapon1'): AugmentUsage => ({ catalog: globalCatalog, items, slot });
const globalCandidate = () => info({ options: globalCatalog });

test('a limited rune in the other weapon blocks inheritance and manual or as-listed duplicates', () => {
  const usage = usageFor([{ slot: 'weapon2', text: 'Rune: Rune of Vitality' }]);
  const current = info({ sockets: 2, runes: ['Rune of Vitality', 'Damage Rune'] });
  const plan = planReplacementAugments(globalCandidate(), current, 72, { mode: 'inherit' }, usage);
  expect(plan.runes).toEqual(['Damage Rune']);
  expect(plan.skipped).toEqual(['Rune of Vitality']);
  expect(plan.info.options.find(entry => entry.name === vitality.name)?.limit).toBe(0);
  expect(() => planReplacementAugments(globalCandidate(), undefined, 72,
    { mode: 'custom', sockets: 1, runes: [vitality.name] }, usage)).toThrow('augment-limit');
  const listed = { ...globalCandidate(), sockets: 1, runes: [vitality.name] };
  for (const mode of ['original', 'inherit'] as const) {
    expect(() => planReplacementAugments(listed, undefined, 72, { mode }, usage)).toThrow('augment-limit');
  }
});

test('replacing a position releases its old use and only the supplied active equipment is counted', () => {
  const active = [{ slot: 'weapon1', text: 'Rune: Rune of Vitality' }, { slot: 'weapon2', text: 'Rune: Damage Rune' }];
  const inactive = [{ slot: 'weapon1', text: 'Rune: Rune of Vitality' }];
  const usage = usageFor(active);
  const candidate = globalCandidate(), original = structuredClone({ candidate, usage, inactive });
  const plan = planReplacementAugments(candidate, info({ sockets: 1, runes: [vitality.name] }), 72, { mode: 'inherit' }, usage);
  expect(plan.runes).toEqual([vitality.name]);
  expect(plan.info.options.find(entry => entry.name === vitality.name)?.limit).toBe(1);
  expect({ candidate, usage, inactive }).toEqual(original);
});

test('different augment names share a limit across items and within one item', () => {
  const usage = usageFor([{ slot: 'helmet', text: `Idol: ${serpent.name}` }]);
  const plan = planReplacementAugments(globalCandidate(), info({ sockets: 1, runes: [fox.name] }), 72, { mode: 'inherit' }, usage);
  expect(plan.runes).toEqual([]);
  expect(plan.skipped).toEqual([fox.name]);
  expect(plan.info.options.filter(entry => entry.limit_id === 'AncientAugment').map(entry => entry.limit)).toEqual([0, 0]);
  expect(() => planReplacementAugments(globalCandidate(), undefined, 72,
    { mode: 'custom', sockets: 2, runes: [serpent.name, fox.name] }, usageFor([]))).toThrow('augment-limit');
  const emptyUsagePlan = planReplacementAugments(globalCandidate(), info({ sockets: 2, runes: [serpent.name, fox.name] }), 72,
    { mode: 'inherit' }, usageFor([]));
  expect(emptyUsagePlan.runes).toEqual([serpent.name]);
  expect(emptyUsagePlan.skipped).toEqual([fox.name]);
});

test('English and localized names are exact aliases while empty socket markers and notes never consume limits', () => {
  for (const name of [vitality.name, vitality.name_zh_cn!, vitality.name_zh_tw!]) {
    const remaining = applyEquippedAugmentLimits(globalCandidate(), usageFor([{ slot: 'helmet', text: `Soul Core: ${name}` }]));
    expect(remaining.info.options.find(entry => entry.name === vitality.name)?.limit).toBe(0);
    expect(remaining.limitWarning).toBeUndefined();
  }
  const empty = applyEquippedAugmentLimits(globalCandidate(), usageFor([{ slot: 'helmet',
    text: 'Rune: None\nSoul Core: \nIdol: none\nNote: Rune: Rune of Vitality\nRune:\n+50 to maximum Life' }]));
  expect(empty.info.options.find(entry => entry.name === vitality.name)?.limit).toBe(1);
  expect(empty.limitWarning).toBeUndefined();
});

test('unknown names or orphan rune effects conservatively disable limited additions without blocking ordinary runes', () => {
  for (const text of ['Rune: Uncatalogued Rune', '{rune}+40 to maximum Life', 'Rune: None\n{range:0.5}{rune}+40 to maximum Life']) {
    const plan = planReplacementAugments(globalCandidate(), info({ sockets: 2, runes: [vitality.name, 'Damage Rune'] }), 72,
      { mode: 'inherit' }, usageFor([{ slot: 'helmet', text }]));
    expect(plan.limitWarning).toBe('unknown-equipped-augments');
    expect(plan.runes).toEqual(['Damage Rune']);
    expect(plan.skipped).toEqual([vitality.name]);
    expect(plan.info.options.find(entry => entry.name === 'Damage Rune')?.limit).toBeUndefined();
    expect(() => planReplacementAugments(globalCandidate(), undefined, 72,
      { mode: 'custom', sockets: 1, runes: [vitality.name] }, usageFor([{ slot: 'helmet', text }]))).toThrow('augment-limit');
  }
});

test('the global rule remains authoritative for readonly existing items and the editor can inspect invalid setups without throwing', () => {
  const usage = usageFor([{ slot: 'weapon2', text: `Rune: ${vitality.name}` }]);
  const readonly = info({ editable: false, sockets: 1, runes: [vitality.name], options: [] });
  expect(() => planReplacementAugments(readonly, undefined, 72, { mode: 'original' }, usage)).toThrow('augment-limit');
  const editable = info({ sockets: 1, runes: [vitality.name], options: [{ ...vitality, limit: 3 }] });
  const adjusted = applyEquippedAugmentLimits(editable, usage);
  expect(adjusted.info.runes).toEqual([vitality.name]);
  expect(adjusted.info.options[0].limit).toBe(0);
  expect(editable.options[0].limit).toBe(3);
  const twoAllowed = { ...usage, catalog: [{ ...vitality, limit: 2 }] };
  const once = applyEquippedAugmentLimits(editable, twoAllowed);
  expect(once.info.options[0].limit).toBe(1);
  expect(applyEquippedAugmentLimits(once.info, twoAllowed).info.options[0].limit).toBe(1);
});
