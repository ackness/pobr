import { expect, test, vi } from 'vitest';
import { compareReplacement, compareReplacementPositions, normalizeCopiedItem, replacementAffixes, validateReplacement } from './itemReplacement';
import type { TradeCatalog } from './tradeOptimizer';
import type { CalculateBuildRequest, ItemAugmentInfo, RuneCatalogEntry } from '../api/types';
import type { EvaluateOptions, EvaluateResult } from './optimize';

vi.mock('../api/backend', () => ({ getBackend: async () => ({
  itemAugmentInfo: async () => ({ sockets: 0, max_sockets: 0, runes: [], editable: false, options: [] }),
  reforgeRunes: async (text: string) => text,
  runeCatalog: async () => [],
}) }));

const catalog: TradeCatalog = { mods: [], bases: [
  { name: 'Sapphire Ring', category: 'accessory.ring', tags: [], level: 12, implicits: [] },
  { name: 'Twin Bow', category: 'weapon.bow', tags: [], level: 55, implicits: [] },
  { name: 'Primed Quiver', category: 'armour.quiver', tags: [], level: 30, implicits: [] },
  { name: 'Ruby', category: 'jewel', tags: [], level: 1, implicits: [] },
] };
const copied = 'Rarity: RARE\nUpgrade\nSapphire Ring\n--------\nRequirements:\nLevel: 60\nDex: 90\n--------\nItem Level: 100\n--------\n+40 to maximum Life';
const request: CalculateBuildRequest = { character: { level: 72, class_name: 'Ranger' },
  items: [{ slot: 'ring1', text: 'Rarity: NORMAL\nSapphire Ring' }, { slot: 'ring2', text: 'Keep this ring' }],
  allocated_nodes: [12], jewels: [{ socket_node: 12, text: 'Keep this jewel' }],
};
const translate = async (lines: string[]) => lines.map(line => line === 'Sapphire Ring' ? '蓝玉戒指' : line);

test('clipboard CN headers retain effects and map short attribute requirements', () => {
  const normalized = normalizeCopiedItem('物品类别: 戒指\n稀有度：稀有\n升级\n蓝玉戒指\n--------\n需求等级: 60\n力量: 10\n敏捷: 20\n智慧: 30\n--------\n+40 最大生命');
  expect(normalized).toContain('Rarity: RARE\n升级\n蓝玉戒指');
  expect(normalized).toContain('Level: 60\nStr: 10\nDex: 20\nInt: 30');
  expect(normalized).toContain('+40 最大生命');
});

test('legacy userscript pseudo mods become metadata while genuine unknown effects remain visible', () => {
  const raw = `${copied}\nUnmodeled market effect (pseudoMods): Sum: 369.1\nUnmodeled market effect (pseudoMods): +120% total Elemental Resistance\nUnmodeled market effect (unknownMods): +50 to maximum Life\nSum: custom text to review`;
  const normalized = normalizeCopiedItem(raw);
  expect(normalized).toContain('Note: Sum: 369.1');
  expect(normalized).toContain('Note: +120% total Elemental Resistance');
  expect(normalized).not.toContain('Unmodeled market effect (pseudoMods)');
  expect(normalized).toContain('Unmodeled market effect (unknownMods): +50 to maximum Life');
  expect(normalized).toContain('Sum: custom text to review');
  expect(normalized).toContain('+40 to maximum Life');
});

test('required level uses declared/base requirements, not item level', async () => {
  await expect(validateReplacement(request, 'ring1', copied, catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', copied.replace('Level: 60', 'Level: 80'), catalog, translate)).rejects.toThrow('item-level');
  await expect(validateReplacement({ ...request, character: { level: 1 } }, 'ring1', 'Rarity: NORMAL\nSapphire Ring', catalog, translate)).rejects.toThrow('item-level');
});

test('unknown bases, wrong slots, inactive jewel sockets and unidentified items never reach calc', async () => {
  for (const [slot, text, error] of [
    ['helmet', copied, 'wrong-slot'], ['ring1', 'Rarity: RARE\nSapphire Ring\nTwin Bow', 'wrong-slot'], ['ring1', copied.replace('Sapphire Ring', 'Unknown'), 'unknown-base'],
    ['Jewel@13', 'Rarity: MAGIC\nRuby', 'wrong-slot'], ['ring1', `${copied}\nUnidentified`, 'unidentified-item'],
  ]) {
    const evaluate = vi.fn();
    await expect(compareReplacement(request, slot, text, undefined, { catalog, translate, evaluate })).rejects.toThrow(error);
    expect(evaluate).not.toHaveBeenCalled();
  }
});

test('magic and localized base names are verified without interpreting modifier lines as bases', async () => {
  await expect(validateReplacement(request, 'ring1', 'Rarity: MAGIC\nHealthy Sapphire Ring of the Beast', catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', copied.replace('Sapphire Ring', '蓝玉戒指'), catalog, translate)).resolves.toEqual(catalog.bases[0]);
  await expect(validateReplacement(request, 'ring1', 'Rarity: RARE\nFake\nUnknown\n--------\nSapphire Ring', catalog, translate)).rejects.toThrow('unknown-base');
});

test('quiver needs a bow and coordinated weapon changes are rejected', async () => {
  await expect(validateReplacement(request, 'weapon2', 'Rarity: NORMAL\nPrimed Quiver', catalog, translate)).rejects.toThrow('weapon-type');
  await expect(validateReplacement({ ...request, items: [{ slot: 'weapon1', text: 'Rarity: NORMAL\nTwin Bow' }] }, 'weapon2', 'Rarity: NORMAL\nPrimed Quiver', catalog, translate)).resolves.toEqual(catalog.bases[2]);
});

test('preview substitutes only the requested position and preserves the source build', async () => {
  const original = structuredClone(request);
  const evaluate = vi.fn(async () => ({ baseline: { TotalDPS: 100, TotalEHP: 1000 },
    results: [{ index: 0, label: null, stats: { TotalDPS: 90, TotalEHP: 1200 }, unsupported: ['Guard'], error: null }], aborted: false }));
  const result = await compareReplacement(request, 'ring1', copied, undefined, { catalog, translate, evaluate });
  expect(evaluate).toHaveBeenCalledWith({ request, signal: undefined, variants: [{ set_items: [{ slot: 'ring1', text: copied }] }] });
  expect(result.stats.TotalDPS).toBe(90);
  expect(result.unsupported).toEqual(['Guard']);
  expect(request).toEqual(original);
});

test('cancelled comparisons do not evaluate or return stale results', async () => {
  const controller = new AbortController(); controller.abort();
  const evaluate = vi.fn();
  await expect(compareReplacement(request, 'ring1', copied, controller.signal, { catalog, translate, evaluate })).rejects.toThrow();
  expect(evaluate).not.toHaveBeenCalled();
});

const classify = vi.fn(async () => [
  { kind: 'name' as const, text: 'Upgrade' }, { kind: 'base' as const, text: 'Sapphire Ring' },
  { kind: 'struct' as const, text: 'Item Level: 100' }, { kind: 'struct' as const, text: '--------' },
  { kind: 'explicit' as const, text: '+40 to maximum Life' },
]);
const evaluatePositions = async ({ variants }: EvaluateOptions): Promise<EvaluateResult> => ({
  baseline: { TotalDPS: 100, TotalEHP: 1000 }, aborted: false,
  results: variants.map((_, index) => ({ index, label: null, error: null,
    stats: { TotalDPS: index ? 110 : 90, TotalEHP: 1000 }, unsupported: [] })),
});

test('one copied ring compares both occupied ring positions in one batch without modifying the build', async () => {
  const original = structuredClone(request);
  const evaluate = vi.fn(evaluatePositions);
  const result = await compareReplacementPositions(request, copied, undefined, { catalog, translate, classify, evaluate });
  expect(result.positions.map(row => row.slot)).toEqual(['ring1', 'ring2']);
  expect(result.positions.map(row => row.stats.TotalDPS)).toEqual([90, 110]);
  expect(result.requiredLevel).toBe(60);
  expect(result.lines).toEqual(await classify());
  expect(evaluate).toHaveBeenCalledTimes(1);
  expect(evaluate.mock.calls[0][0].variants).toEqual([
    { set_items: [{ slot: 'ring1', text: copied }] }, { set_items: [{ slot: 'ring2', text: copied }] },
  ]);
  expect(result.positions[1].variant).toEqual(evaluate.mock.calls[0][0].variants[1]);
  expect(request).toEqual(original);
});

test('jewel discovery includes allocated empty sockets and never treats ordinary nodes or unallocated sockets as destinations', async () => {
  const build = { ...request, allocated_nodes: [12, 13, 16] };
  const result = await compareReplacementPositions(build, 'Rarity: MAGIC\nRuby', undefined,
    { catalog, translate, classify, evaluate: evaluatePositions, jewelSocketNodes: [12, 13, 14] });
  expect(result.positions.map(row => row.slot)).toEqual(['Jewel@12', 'Jewel@13']);
  expect(result.positions[1].variant.jewels).toEqual([...request.jewels!, { socket_node: 13, text: 'Rarity: MAGIC\nRuby' }]);
});

test('failed destinations remain explained while valid destination results stay available', async () => {
  const result = await compareReplacementPositions(request, copied, undefined, { catalog, translate, classify,
    evaluate: async options => { const result = await evaluatePositions(options); result.results[0].error = 'Cannot calculate this ring position'; return result; } });
  expect(result.positions.map(row => row.slot)).toEqual(['ring2']);
  expect(result.rejected).toEqual([{ slot: 'ring1', reason: 'Cannot calculate this ring position' }]);
});

test('missing positions, changed weapon family and excessive requirements explain why no comparison is possible', async () => {
  const evaluate = vi.fn(evaluatePositions);
  await expect(compareReplacementPositions({ character: { level: 85 } }, copied, undefined,
    { catalog, translate, classify, evaluate })).rejects.toThrow('no-compatible-slot');
  const expanded = { ...catalog, bases: [...catalog.bases, { name: 'Ashen Staff', category: 'weapon.staff', level: 1, tags: [], implicits: [] }] };
  const staffBuild = { ...request, items: [{ slot: 'weapon1', text: 'Rarity: NORMAL\nAshen Staff' }] };
  await expect(compareReplacementPositions(staffBuild, 'Rarity: NORMAL\nTwin Bow', undefined,
    { catalog: expanded, translate, classify, evaluate })).rejects.toThrow('weapon-type');
  await expect(compareReplacementPositions(request, copied.replace('Requirements:\nLevel: 60', 'Requires Level 80'), undefined,
    { catalog, translate, classify, evaluate })).rejects.toThrow('item-level');
  expect(evaluate).not.toHaveBeenCalled();
});

test('Chinese combined requirements and separate magic clipboard bases are recognized', async () => {
  const cn = normalizeCopiedItem('稀有度: 稀有\n升级\n蓝玉戒指\n--------\n需求：等级80，90敏捷\n--------\n+40 最大生命');
  await expect(validateReplacement(request, 'ring1', cn, catalog, translate)).rejects.toThrow('item-level');
  await expect(validateReplacement(request, 'ring1', 'Rarity: MAGIC\nMagic Name\nSapphire Ring\n--------\n+40 to maximum Life', catalog, translate)).resolves.toEqual(catalog.bases[0]);
  expect(() => normalizeCopiedItem(`${copied}\n${copied}`)).toThrow('invalid-item');
});

test('parsed affix lists exclude metadata and equip restrictions even when legacy classifier tags are imprecise', () => {
  expect(replacementAffixes([
    { kind: 'name', text: 'Name' }, { kind: 'base', text: 'Sapphire Ring' }, { kind: 'struct', text: 'Level: 60' },
    { kind: 'explicit', text: '--------' }, { kind: 'explicit', text: 'Quality: 20%' },
    { kind: 'class_req', text: 'Requires a special ascendancy' }, { kind: 'implicit', text: '+30% to Cold Resistance' },
    { kind: 'explicit', text: '+40 to maximum Life' }, { kind: 'rune', text: '+20% to Fire Resistance' },
  ]).map(row => row.text)).toEqual(['+30% to Cold Resistance', '+40 to maximum Life', '+20% to Fire Resistance']);
});

test('cancellation during parsing never sends the item to the calculator', async () => {
  const controller = new AbortController();
  const evaluate = vi.fn(evaluatePositions);
  await expect(compareReplacementPositions(request, copied, controller.signal, { catalog, translate, evaluate,
    classify: async () => { controller.abort(); return []; } })).rejects.toThrow();
  expect(evaluate).not.toHaveBeenCalled();
});

const augmentOption = (name: string): RuneCatalogEntry => ({ name, name_zh_cn: null, name_zh_tw: null,
  is_soul_core: false, kind: 'Rune', required_level: 1, lines: ['Synthetic effect'] });
const augmentOptions = [augmentOption('Damage Rune'), augmentOption('Life Rune')];
const augmentState = (sockets = 0, runes: string[] = []): ItemAugmentInfo => ({ sockets, max_sockets: 2,
  runes, editable: true, options: augmentOptions });
const maceCatalog: TradeCatalog = { mods: [], bases: [
  { name: 'Synthetic Mace', category: 'weapon.onemace', tags: [], level: 1, implicits: [] },
] };
const maceCandidate = 'Rarity: RARE\nMarket Mace\nSynthetic Mace\n--------\nImplicits: 0\n--------\n20% increased Physical Damage';
const dualBuild: CalculateBuildRequest = { character: { level: 72, class_name: 'Warrior' }, items: [
  { slot: 'weapon1', text: 'Rarity: RARE\nCurrent One\nSynthetic Mace\n--------\nSockets: S\nRune: Damage Rune' },
  { slot: 'weapon2', text: 'Rarity: RARE\nCurrent Two\nSynthetic Mace\n--------\nSockets: S S\nRune: Life Rune\nRune: Damage Rune' },
  { slot: 'helmet', text: 'Preserve unrelated helmet' },
] };
const sourceInfo = async (text: string): Promise<ItemAugmentInfo> => text.includes('Current One')
  ? augmentState(1, ['Damage Rune']) : text.includes('Current Two') ? augmentState(2, ['Life Rune', 'Damage Rune']) : augmentState();
const reforgeText = async (text: string, runes: string[], sockets?: number) => text.replace('Implicits: 0',
  [`Sockets: ${Array(sockets ?? 0).fill('S').join(' ')}`, ...runes.filter(Boolean).map(name => `Rune: ${name}`), 'Implicits: 0'].join('\n'));
const classifyAugments = async (text: string) => text.split('\n').filter(line => line.startsWith('Rune:'))
  .map(line => ({ kind: 'rune' as const, text: line.slice(6) }));

test('each compatible position inherits its own current augments and exposes the exact evaluated variant for apply', async () => {
  const original = structuredClone(dualBuild);
  const reforge = vi.fn(reforgeText), evaluate = vi.fn(evaluatePositions);
  const report = await compareReplacementPositions(dualBuild, maceCandidate, undefined, {
    catalog: maceCatalog, translate, classify: classifyAugments, augmentInfo: sourceInfo, reforge, evaluate,
  });
  expect(reforge.mock.calls).toEqual([
    [maceCandidate, ['Damage Rune'], 1], [maceCandidate, ['Life Rune', 'Damage Rune'], 2],
  ]);
  expect(report.positions.map(row => row.augments.runes)).toEqual([['Damage Rune'], ['Life Rune', 'Damage Rune']]);
  const evaluated = evaluate.mock.calls[0][0].variants;
  expect(evaluated).toHaveLength(2);
  for (const [index, row] of report.positions.entries()) {
    expect(row.variant).toEqual(evaluated[index]);
    expect(row.variant.set_items?.[0].slot).toBe(index ? 'weapon2' : 'weapon1');
    expect(row.variant.set_items?.[0].text).toContain('20% increased Physical Damage');
    expect(row.lines.map(line => line.text)).toEqual(row.augments.runes);
    expect(row.augments.source).toBe('inherited');
  }
  expect(report.text).toBe(maceCandidate);
  expect(dualBuild).toEqual(original);
});

test('listing augments and explicit as-listed choices are never reforged or replaced by current gear', async () => {
  for (const asListed of [false, true]) {
    const reforge = vi.fn(reforgeText);
    const augmentInfo = vi.fn(async () => asListed ? augmentState() : augmentState(2, ['', 'Life Rune']));
    const report = await compareReplacementPositions(dualBuild, maceCandidate, undefined, {
      catalog: maceCatalog, translate, classify: classifyAugments, augmentInfo, reforge, evaluate: evaluatePositions,
      augmentPlans: asListed ? { weapon1: { mode: 'original' }, weapon2: { mode: 'original' } } : undefined,
    });
    expect(reforge).not.toHaveBeenCalled();
    expect(augmentInfo).toHaveBeenCalledTimes(1);
    expect(report.positions.every(row => row.augments.source === 'original')).toBe(true);
    expect(report.positions.map(row => row.variant.set_items?.[0].text)).toEqual([maceCandidate, maceCandidate]);
  }
});

test('existing listing augments enforce their required level even when the copied requirements are missing', async () => {
  const text = maceCandidate.replace('Implicits: 0', 'Sockets: S\nRune: Perfect Rune\nImplicits: 0');
  const augmentInfo = vi.fn(async (): Promise<ItemAugmentInfo> => ({ ...augmentState(1, ['Perfect Rune']),
    options: [{ ...augmentOption('Perfect Rune'), required_level: 80 }] }));
  const evaluate = vi.fn(evaluatePositions), reforge = vi.fn(reforgeText);
  const dependencies = { catalog: maceCatalog, translate, classify: classifyAugments, augmentInfo, reforge, evaluate };
  await expect(compareReplacementPositions(dualBuild, text, undefined, dependencies)).rejects.toThrow('item-level');
  expect(evaluate).not.toHaveBeenCalled();
  expect(reforge).not.toHaveBeenCalled();
  const report = await compareReplacementPositions({ ...dualBuild, character: { level: 80, class_name: 'Warrior' } }, text, undefined, dependencies);
  expect(report.requiredLevel).toBe(80);
  expect(report.positions.every(row => row.augments.source === 'original')).toBe(true);
  expect(report.positions.every(row => row.variant.set_items?.[0].text === text)).toBe(true);
  expect(reforge).not.toHaveBeenCalled();
});

test('whole-build augment limits reject only destinations that would duplicate an occupied limited rune', async () => {
  const vitality = { ...augmentOption('Rune of Vitality'), limit: 1 };
  const text = maceCandidate.replace('Implicits: 0', 'Sockets: S\nRune: Rune of Vitality\nImplicits: 0');
  const build = { ...dualBuild, items: dualBuild.items!.map(item => ({ ...item,
    text: item.text.replace('Rune: Damage Rune', 'Rune: Rune of Vitality') })) };
  // Only weapon1 occupies the limited rune; replacing weapon2 would leave it equipped.
  build.items[1].text = dualBuild.items![1].text;
  const evaluate = vi.fn(evaluatePositions), reforge = vi.fn(reforgeText);
  const report = await compareReplacementPositions(build, text, undefined, {
    catalog: maceCatalog, translate, classify: classifyAugments, evaluate, reforge,
    augmentInfo: async () => ({ ...augmentState(1, [vitality.name]), options: [vitality, ...augmentOptions] }),
    runeCatalog: async () => [vitality, ...augmentOptions],
  });
  expect(report.positions.map(row => row.slot)).toEqual(['weapon1']);
  expect(report.rejected).toEqual([{ slot: 'weapon2', reason: 'augment-limit' }]);
  expect(report.positions[0].augments.info.options.find(entry => entry.name === vitality.name)?.limit).toBe(1);
  expect(evaluate.mock.calls[0][0].variants).toEqual([{ set_items: [{ slot: 'weapon1', text }] }]);
  expect(reforge).not.toHaveBeenCalled();
});

test('custom preparation is per-position and its empty sockets stay in the evaluated apply payload', async () => {
  const reforge = vi.fn(reforgeText);
  const report = await compareReplacementPositions(dualBuild, maceCandidate, undefined, {
    catalog: maceCatalog, translate, classify: classifyAugments, augmentInfo: sourceInfo, reforge, evaluate: evaluatePositions,
    augmentPlans: { weapon1: { mode: 'custom', sockets: 2, runes: ['', 'Life Rune'] }, weapon2: { mode: 'original' } },
  });
  expect(reforge.mock.calls).toEqual([[maceCandidate, ['', 'Life Rune'], 2]]);
  expect(report.positions[0].augments).toMatchObject({ source: 'custom', sockets: 2, runes: ['', 'Life Rune'] });
  expect(report.positions[0].variant.set_items?.[0].text).toContain('Sockets: S S\nRune: Life Rune');
  expect(report.positions[1].variant.set_items?.[0].text).toBe(maceCandidate);
});

test('a preparation failure preserves the next compatible position and its remapped calculator index', async () => {
  const evaluate = vi.fn(evaluatePositions);
  const report = await compareReplacementPositions(dualBuild, maceCandidate, undefined, {
    catalog: maceCatalog, translate, classify: classifyAugments, augmentInfo: sourceInfo, evaluate,
    reforge: async (text, runes, sockets) => {
      if (runes.length === 1) throw new Error('Cannot prepare this augment setup');
      return reforgeText(text, runes, sockets);
    },
  });
  expect(report.rejected).toEqual([{ slot: 'weapon1', reason: 'Cannot prepare this augment setup' }]);
  expect(report.positions).toHaveLength(1);
  expect(report.positions[0].slot).toBe('weapon2');
  expect(report.positions[0].variant.set_items?.[0].slot).toBe('weapon2');
  expect(report.positions[0].stats.TotalDPS).toBe(90);
  expect(evaluate.mock.calls[0][0].variants).toEqual([report.positions[0].variant]);
});

test('aborting metadata lookup or reforge preparation never sends partial variants to calc', async () => {
  for (const stage of ['metadata', 'reforge']) {
    const controller = new AbortController();
    const evaluate = vi.fn(evaluatePositions);
    await expect(compareReplacementPositions(dualBuild, maceCandidate, controller.signal, {
      catalog: maceCatalog, translate, classify: classifyAugments, evaluate,
      augmentInfo: async text => { if (stage === 'metadata' && text.includes('Current')) controller.abort(); return sourceInfo(text); },
      reforge: async (text, runes, sockets) => { if (stage === 'reforge') controller.abort(); return reforgeText(text, runes, sockets); },
    })).rejects.toThrow();
    expect(evaluate).not.toHaveBeenCalled();
  }
});
