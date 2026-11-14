import { afterEach, expect, test, vi } from 'vitest';
import { EXPECTED_SCHEMA_VERSION } from './types';
import { createWasmBackend } from './wasmBackend';
import type { GemCatalogEntry } from './types';
import { classDisplayName } from '../lib/nativeDisplayNames';
import { gemDisplayName } from '../lib/skillNames';
import { createHash } from 'node:crypto';

const state = vi.hoisted(() => ({ gems: [] as GemCatalogEntry[], crafting: { bases: [] as unknown[], mods: [] as unknown[] },
  detailed: false, ready: true }));

// Match the runtime import URL even when no local WASM package exists.
vi.mock('/src/wasm/pkg/pobr_wasm.js', () => ({
  default: async () => {}, schemaVersion: () => EXPECTED_SCHEMA_VERSION,
  gemCatalogJson: () => JSON.stringify(state.gems),
  itemCraftingCatalogJson: () => JSON.stringify(state.crafting),
  isDataReady: () => state.ready,
  stageDataBytes: () => {},
  initStagedDataVerified: () => { state.ready = true; },
  dataInfo: () => JSON.stringify({ data_version: '1.0.0.1', tree_version: 'future_tree', locales: [
    { code: 'zh-CN', fallback: [], chain: ['zh-CN'], templates: true },
    { code: 'zh-TW', fallback: [], chain: ['zh-TW'], templates: false },
  ] }),
  translateLines: (lang: string, raw: string) =>
    JSON.stringify((JSON.parse(raw) as string[]).map(() => lang === 'zh-CN' ? '旧译文' : 'wrong locale')),
  // Present only when a test turns it on, as in a WASM build that has the export.
  get translateLinesDetailed() {
    return state.detailed ? (lang: string, raw: string) => JSON.stringify((JSON.parse(raw) as string[]).map(line =>
      ({ text: lang === 'zh-CN' ? '旧译文' : 'wrong locale', origin: line.startsWith('Ours') ? 'agent' : null }))) : undefined;
  },
}));

afterEach(() => { vi.unstubAllGlobals(); state.gems = []; state.crafting = { bases: [], mods: [] }; state.detailed = false; state.ready = true; });

test('passive tree loading uses the WASM label for a 1.0 pack and keeps historical labels', async () => {
  const current = [{ skill: 1 }], historical = [{ skill: 2 }];
  const files = { 'base/passive_tree.json': current, 'base/passive_trees/0_4.json': historical };
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: '1.0.0.1', files: Object.keys(files) }));
    const file = url.replace('/data/1.0.0.1/', '') as keyof typeof files;
    if (!(file in files)) throw new Error(`Unexpected fetch: ${url}`);
    return new Response(JSON.stringify(files[file]));
  });
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  expect(await backend.loadPassiveTree('future_tree')).toEqual(current);
  expect(await backend.loadPassiveTree('Default')).toEqual(current);
  expect(await backend.loadPassiveTree('0_4')).toEqual(historical);
  expect(backend.dataInfo).toMatchObject({ data_version: '1.0.0.1', tree_version: 'future_tree' });
  await expect(backend.loadPassiveTree('unregistered')).rejects.toThrow('unavailable');
  expect(fetch.mock.calls.some(([url]) => url.includes('0_0'))).toBe(false);
});

test('enemy presets use the pinned snapshot inventory and the shared file cache', async () => {
  const path = 'base/enemy_presets.json';
  const table = { max_enemy_level: 85, tiers: [] };
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'snapshot', files: [path], digests: { [path]: 'a'.repeat(64) } }));
    if (url === `/data/h/${'a'.repeat(16)}/${path}`) return new Response(JSON.stringify(table));
    throw new Error(`Unexpected fetch: ${url}`);
  });
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  const [first, second] = await Promise.all([backend.loadEnemyPresets(), backend.loadEnemyPresets()]);
  expect(first).toEqual(table);
  expect(second).toEqual(table);
  expect(fetch.mock.calls.filter(([url]) => url.endsWith(path))).toHaveLength(1);
});

test('an unlisted enemy preset file stays unavailable instead of fetching outside the snapshot', async () => {
  const fetch = vi.fn(async () => new Response(JSON.stringify({ version: 'snapshot', files: [] })));
  vi.stubGlobal('fetch', fetch);
  expect(await (await createWasmBackend()).loadEnemyPresets()).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test('detailed lines and names carry supplement origins, with the same texts as the plain calls', async () => {
  vi.stubGlobal('fetch', async (url: string) => new Response(JSON.stringify(url === '/data/manifest.json'
    ? { version: 'test', files: [] } : null)));
  // A WASM build without the export: every text is official.
  let backend = await createWasmBackend();
  expect(await backend.translateLinesDetailed(['Ours line', 'Other'], 'zh-CN'))
    .toEqual([{ text: '旧译文', origin: null }, { text: '旧译文', origin: null }]);
  state.detailed = true;
  backend = await createWasmBackend();
  expect(await backend.translateLinesDetailed(['Ours line', 'Other'], 'zh-CN'))
    .toEqual([{ text: '旧译文', origin: 'agent' }, { text: '旧译文', origin: null }]);
  // zh-TW renders through zh-CN and keeps the origin.
  expect(await backend.translateLinesDetailed(['Ours line'], 'zh-TW')).toEqual([{ text: '舊譯文', origin: 'agent' }]);
  expect(await backend.translateLines(['Ours line', 'Other'], 'zh-CN')).toEqual(['旧译文', '旧译文']);
  expect(await backend.translateLinesDetailed(['Ours line'], 'en-US')).toEqual([{ text: 'Ours line', origin: null }]);
  expect(await backend.translateNamesDetailed([{ kind: 'passive', id: 'p', name: 'Ours name' },
    { kind: 'monster', id: 'Metadata/Monsters/A', name: 'Zombie' }], 'zh-CN'))
    .toEqual([{ text: '旧译文', origin: 'agent' }, { text: 'Zombie', origin: null }]);
});

test('crafting metadata uses the listed scoped base catalog without changing its identity', async () => {
  state.crafting.bases = [{ id: 'base', name: 'Same', item_class: 'Ring', tags: [], mod_domain: 1 }];
  const sidecar = { schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international',
    locale: 'zh-CN', catalog_sha256: 'a'.repeat(64), entries: { Same: null },
    scopes: { 'BaseItemTypes.Name': { base: ['Same', '戒指'] } } };
  vi.stubGlobal('fetch', async (url: string) => new Response(JSON.stringify(url === '/data/manifest.json'
    ? { version: 'test', files: ['i18n/zh-CN/display_texts.json'] } : sidecar)));
  const backend = await createWasmBackend();
  expect((await backend.itemCraftingCatalog()).bases[0]).toEqual({ ...state.crafting.bases[0] as object,
    display_names: { 'zh-CN': '戒指' } });
  expect(await backend.translateLines(['Same'], 'zh-CN')).toEqual(['Same']);
});

test('the ordinary backend loads only listed native display sidecars and preserves English on unresolved records', async () => {
  const requests: string[] = [];
  const sidecar = {
    schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale: 'zh-TW',
    catalog_sha256: 'a'.repeat(64), entries: { 'Native class': '原生職業', 'Unresolved class': null }, scopes: {},
  };
  vi.stubGlobal('fetch', async (url: string) => {
    requests.push(url);
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: ['i18n/zh-TW/display_texts.json'] }));
    if (url === '/data/test/i18n/zh-TW/display_texts.json') return new Response(JSON.stringify(sidecar));
    throw new Error(`Unexpected fetch: ${url}`);
  });
  const backend = await createWasmBackend();
  expect(await backend.translateLines(['Native class', 'Unresolved class'], 'zh-TW')).toEqual(['原生職業', 'Unresolved class']);
  expect(await backend.translateLines(['Native class'], 'en-US')).toEqual(['Native class']);
  expect(await backend.translateLines(['Native class'], 'zh-CN')).toEqual(['旧译文']);
  // zh-TW has no templates of its own: its lines render through zh-CN, then in its script.
  expect(await backend.translateLines(['Other line'], 'zh-TW')).toEqual(['舊譯文']);
  expect(requests).toEqual(['/data/manifest.json', '/data/test/i18n/zh-TW/display_texts.json']);
});

test('a wrong snapshot sidecar is reported, never silently used as another version', async () => {
  vi.stubGlobal('fetch', async (url: string) => new Response(JSON.stringify(url === '/data/manifest.json'
    ? { version: 'test', files: ['i18n/zh-CN/display_texts.json'] }
    : { schema: 'pobr-native-display-texts/v1', game_patch: 'other', locale: 'zh-CN' })));
  const backend = await createWasmBackend();
  await expect(backend.translateLines(['Name'], 'zh-CN')).rejects.toThrow(/identity/);
});

test('a listed null sidecar is invalid rather than treated as an absent optional file', async () => {
  vi.stubGlobal('fetch', async (url: string) => new Response(JSON.stringify(url === '/data/manifest.json'
    ? { version: 'test', files: ['i18n/zh-CN/display_texts.json'] } : null)));
  const backend = await createWasmBackend();
  await expect(backend.translateLines(['Name'], 'zh-CN')).rejects.toThrow(/identity/);
});

test('ordinary gem and class APIs share verified catalogs and source links without rewriting calculation keys', async () => {
  state.gems = [{ skill_id: 'Effect', name: 'Fire', name_zh_cn: '旧火焰', name_zh_tw: '舊火焰',
    active_effect_ids: ['Effect'], colour: 'int', is_support: false, is_lineage: false, is_unique_only: false, tags: ['Spell'] }];
  const files: Record<string, unknown> = {
    'base/skill_gems.json': [{ id: 'Gem/Fire', granted_effect_id: 'OldEffect' }],
    'overlay/gem_effects.json': { gems: [{ gem_id: 'Gem/Fire', granted_effect_id: 'Effect' }] },
    'base/passive_tree_meta.json': { tree: 'default', classes: [{ name: 'Warrior', base_str: 10, base_dex: 5, base_int: 5,
      ascendancies: [{ id: 'Warrior1', name: 'Titan' }] }] },
    'i18n/zh-CN/classes.json': { classes: { Warrior: '旧战士' }, ascendancies: { Titan: '旧泰坦' } },
    'i18n/zh-CN/display_texts.json': {
      schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale: 'zh-CN',
      catalog_sha256: 'a'.repeat(64), entries: { Fire: '烈火', Warrior: null, Titan: '泰坦' },
      scopes: { 'BaseItemTypes.Name': { 'Gem/Fire': ['Fire', '烈火'] },
        'Characters.Name': { Warrior: ['Warrior', null] }, 'Ascendancy.Name': { Warrior1: ['Titan', '泰坦'] } },
    },
  };
  const requests: string[] = [];
  vi.stubGlobal('fetch', async (url: string) => {
    requests.push(url);
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: Object.keys(files) }));
    const file = url.replace('/data/test/', '');
    if (!Object.hasOwn(files, file)) throw new Error(`Unexpected fetch: ${url}`);
    return new Response(JSON.stringify(files[file]));
  });
  const backend = await createWasmBackend();
  const gems = await backend.gemCatalog();
  expect(gemDisplayName(gems[0], 'zh-CN')).toBe('烈火');
  expect(gems[0]).toEqual({ ...state.gems[0], name_zh_cn: '烈火' });
  const names = await backend.loadClassNames();
  expect(classDisplayName(names, 'Warrior', 'zh-CN')).toBe('Warrior');
  expect(classDisplayName(names, 'Titan', 'zh-CN')).toBe('泰坦');
  expect(await backend.translateLines(['Fire', 'Warrior'], 'zh-CN')).toEqual(['烈火', 'Warrior']);
  expect(requests.filter(url => url.endsWith('/display_texts.json'))).toEqual(['/data/test/i18n/zh-CN/display_texts.json']);
});

test('names of known identity use the id-scoped catalog, and every other name the translateLines path', async () => {
  const sidecar = (scopes: Record<string, unknown>) => ({ schema: 'pobr-native-display-texts/v1', game_patch: 'test',
    realm: 'international', locale: 'zh-CN', catalog_sha256: 'a'.repeat(64),
    entries: { 'Mana Regeneration': null, Sapphire: null, 'Seeing Stars': null }, scopes });
  const serve = (display: unknown) => vi.stubGlobal('fetch', async (url: string) => {
    const files: Record<string, unknown> = { 'base/base_items.json': [{ id: 'Metadata/Items/Jewels/JewelInt', name: 'Sapphire' }],
      'i18n/zh-CN/display_texts.json': display };
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: Object.keys(files) }));
    return new Response(JSON.stringify(files[url.replace('/data/test/', '')]));
  });
  const names = [{ kind: 'passive', id: 'mana10', name: 'Mana Regeneration' },
    { kind: 'passive', id: 'energy_shield_recovery10', name: 'Mana Regeneration' },
    { kind: 'passive', id: 'seeing_stars', name: 'Seeing Stars' }, { kind: 'passive', id: 'absent', name: 'Other' },
    { kind: 'base', name: 'Sapphire' }] as const;
  serve(sidecar({ 'BaseItemTypes.Name': { 'Metadata/Items/Jewels/JewelInt': ['Sapphire', '蓝玉'] },
    'PassiveSkills.Name': { mana10: ['Mana Regeneration', '魔力再生'],
      energy_shield_recovery10: ['Mana Regeneration', '魔力回复'], seeing_stars: ['Seeing Stars', null] } }));
  let backend = await createWasmBackend();
  // The unowned name takes the mocked template path ('旧译文'); colliding literals stay English as text.
  expect(await backend.translateNames(names, 'zh-CN')).toEqual(['魔力再生', '魔力回复', 'Seeing Stars', '旧译文', '蓝玉']);
  expect(await backend.translateNames(names, 'en-US')).toEqual(names.map(name => name.name));
  expect(await backend.translateLines(['Sapphire'], 'zh-CN')).toEqual(['Sapphire']);
  // Without the scopes, translateNames is exactly translateLines.
  serve(sidecar({}));
  backend = await createWasmBackend();
  expect(await backend.translateNames(names, 'zh-CN')).toEqual(await backend.translateLines(names.map(name => name.name), 'zh-CN'));
});

test.each([
  { source: 'official', origin: null },
  { source: 'supplement', origin: 'agent' },
] as const)('minion names keep $source origins, script conversion and English fallbacks', async ({ origin }) => {
  const wolfId = 'Metadata/Monsters/Wolves/WolfCompanionPlayerSummoned';
  const sidecar = {
    schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale: 'zh-CN',
    catalog_sha256: 'a'.repeat(64), entries: {}, scopes: {
      'MonsterVarieties.Name': { [wolfId]: ['Wolf Companion', '狼伙伴'] },
      'PassiveSkills.Name': { mana10: ['Mana Regeneration', '魔力再生'] },
    },
    ...(origin ? { origins: { entries: {}, scopes: { 'MonsterVarieties.Name': { [wolfId]: origin } } } } : {}),
  };
  vi.stubGlobal('fetch', async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: ['i18n/zh-CN/display_texts.json'] }));
    if (url === '/data/test/i18n/zh-CN/display_texts.json') return new Response(JSON.stringify(sidecar));
    throw new Error(`Unexpected fetch: ${url}`);
  });
  const backend = await createWasmBackend();
  const requests = [{ kind: 'monster', id: wolfId, name: 'Wolf Companion' },
    { kind: 'monster', name: 'Raging Spirit' }, { kind: 'passive', id: 'mana10', name: 'Mana Regeneration' }] as const;
  // Monster varieties fall back to the CN name; passive names use the locale's line translator.
  for (const [lang, wolf, passive] of [['zh-CN', '狼伙伴', '魔力再生'], ['zh-TW', '狼夥伴', '舊譯文']] as const) {
    const names = await backend.translateNamesDetailed(requests, lang);
    expect(names).toEqual([{ text: wolf, origin }, { text: 'Raging Spirit', origin: null },
      { text: passive, origin: null }]);
    expect(await backend.translateNames(requests, lang)).toEqual(names.map(name => name.text));
  }
  expect(await backend.translateNamesDetailed(requests, 'en-US'))
    .toEqual(requests.map(({ name }) => ({ text: name, origin: null })));
});

test('unlisted class compatibility data is absent while listed invalid data is reported', async () => {
  vi.stubGlobal('fetch', async (url: string) => {
    if (url !== '/data/manifest.json') throw new Error(`Unexpected fetch: ${url}`);
    return new Response(JSON.stringify({ version: 'test', files: [] }));
  });
  expect(await (await createWasmBackend()).loadClassNames()).toEqual({ classes: {}, ascendancies: {} });
  vi.stubGlobal('fetch', async (url: string) => url === '/data/manifest.json'
    ? new Response(JSON.stringify({ version: 'test', files: ['i18n/zh-CN/classes.json'] })) : new Response('invalid JSON'));
  await expect((await createWasmBackend()).loadClassNames()).rejects.toThrow();
});

test('unlisted gem descriptions return null without reading their dependencies', async () => {
  const fetch = vi.fn(async () => new Response(JSON.stringify({ version: 'test', files: ['base/granted_effects.json'] })));
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  expect(fetch).not.toHaveBeenCalled();
  expect(await backend.gemDescriptions()).toBeNull();
  expect(await backend.gemDescriptions()).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test('gem descriptions load lazily, share in-flight work, project effects, and retry after a load error', async () => {
  const files: Record<string, unknown> = {
    'overlay/gem_descriptions.json': { scopes: { 'GemEffects.SupportText': { variant: 'Supports [Spell] Skills.' },
      'ActiveSkills.Description': { active: 'Cast a [Spell].' } } },
    'overlay/gem_effects.json': { gems: [{ gem_id: 'Gem', granted_effect_id: 'Support', variant_id: 'variant' }] },
    'base/granted_effects.json': [{ id: 'Effect', active_skill: 'active', large: 'discard me' }],
    'base/skill_gems.json': [{ id: 'Gem', granted_effect_id: 'OldEffect', int_pct: 100 }],
  };
  let fail = true;
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: Object.keys(files) }));
    const file = url.replace('/data/test/', '');
    if (file === 'overlay/gem_descriptions.json' && fail) return new Response('', { status: 503 });
    if (!Object.hasOwn(files, file)) throw new Error(`Unexpected fetch: ${url}`);
    return new Response(JSON.stringify(files[file]));
  });
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  expect(fetch).not.toHaveBeenCalled();
  await expect(backend.gemDescriptions()).rejects.toThrow(/503/);
  fail = false;
  const [first, second] = await Promise.all([backend.gemDescriptions(), backend.gemDescriptions()]);
  expect(first).toBe(second);
  expect(first?.activeSkills).toEqual({ Effect: 'active' });
  expect(first?.requirements.Support).toEqual(['int']);
  expect(JSON.stringify(first)).not.toContain('discard me');
  expect(fetch.mock.calls.filter(([url]) => url.endsWith('gem_descriptions.json'))).toHaveLength(2);
  expect(await backend.gemDescriptions()).toBe(first);
});

test('item class loading is optional, shares its request, and blocks old text translations once declared', async () => {
  const path = 'overlay/item_class_names.json';
  const sidecar = {
    schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale: 'zh-CN',
    catalog_sha256: 'a'.repeat(64), entries: { Rings: 'wrong text path' },
    scopes: { 'ItemClasses.Name': { Ring: ['Rings', '类别甲'], Other: ['Rings', '类别乙'] } },
    origins: { scopes: { 'ItemClasses.Name': { Ring: 'agent' } } },
  };
  const files: Record<string, unknown> = { 'i18n/zh-CN/display_texts.json': sidecar };
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: Object.keys(files) }));
    const file = url.replace('/data/test/', '');
    if (!Object.hasOwn(files, file)) throw new Error(`Unexpected fetch: ${url}`);
    return new Response(JSON.stringify(files[file]));
  });
  vi.stubGlobal('fetch', fetch);
  const request = { kind: 'itemClass' as const, id: 'Ring', name: 'Rings' };
  let backend = await createWasmBackend();
  // Legacy whole-name matching cannot choose between these two translations.
  expect(await backend.translateNames([request], 'zh-CN')).toEqual(['Rings']);
  expect(fetch.mock.calls.some(([url]) => url.endsWith(path))).toBe(false);
  files[path] = { scopes: { 'ItemClasses.Name': { Ring: 'Rings', Other: 'Rings' } } };
  backend = await createWasmBackend();
  const [first, second] = await Promise.all([
    backend.translateNamesDetailed([request], 'zh-CN'), backend.translateNamesDetailed([request], 'zh-CN'),
  ]);
  expect(first).toEqual([{ text: '类别甲', origin: 'agent' }]);
  expect(second).toEqual(first);
  expect(fetch.mock.calls.filter(([url]) => url.endsWith(path))).toHaveLength(1);
  expect(await backend.translateNames([request, { ...request, id: 'Unknown' }, { ...request, id: undefined }], 'zh-TW'))
    .toEqual(['Rings', 'Rings', 'Rings']);
  expect(await backend.translateNames([request], 'en-US')).toEqual(['Rings']);
});

test.each([null, { scopes: {} }, { scopes: { 'ItemClasses.Name': { Ring: '' } } }])(
  'a declared invalid item class member rejects the name loader: %j', async member => {
    const path = 'overlay/item_class_names.json';
    vi.stubGlobal('fetch', async (url: string) => new Response(JSON.stringify(url === '/data/manifest.json'
      ? { version: 'test', files: [path] } : member)));
    const backend = await createWasmBackend();
    await expect(backend.translateNames([{ kind: 'itemClass', id: 'Ring', name: 'Rings' }], 'zh-CN'))
      .rejects.toThrow(/item class/i);
  });

test('item class fetch failures reject and can retry without becoming a legacy pack', async () => {
  const path = 'overlay/item_class_names.json';
  let fail = true;
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: [path] }));
    return fail ? new Response('', { status: 503 })
      : new Response(JSON.stringify({ scopes: { 'ItemClasses.Name': { Ring: 'Rings' } } }));
  });
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  const request = [{ kind: 'itemClass' as const, id: 'Ring', name: 'Rings' }];
  await expect(backend.translateNames(request, 'zh-CN')).rejects.toThrow(/503/);
  fail = false;
  expect(await backend.translateNames(request, 'zh-CN')).toEqual(['Rings']);
  expect(fetch.mock.calls.filter(([url]) => url.endsWith(path))).toHaveLength(2);
});

test('initialization rejects a declared item class member whose bytes fail its snapshot pin', async () => {
  state.ready = false;
  const path = 'overlay/item_class_names.json';
  const raw = JSON.stringify({ scopes: { 'ItemClasses.Name': { Ring: 'Rings' } } });
  const snapshot = { schema_version: 3, files: { [path]: createHash('sha256').update(raw).digest('hex') } };
  vi.stubGlobal('fetch', async (url: string) => new Response(url === '/data/manifest.json'
    ? JSON.stringify({ version: 'test', files: ['manifest.json', path] })
    : url.endsWith('/manifest.json') ? JSON.stringify(snapshot) : `${raw} `));
  await expect((await createWasmBackend()).init()).rejects.toThrow(`SHA-256 mismatch: ${path}`);
});

test('class name lookup reuses the pinned member bytes staged at initialization', async () => {
  state.ready = false;
  const path = 'overlay/item_class_names.json';
  const contents: Record<string, string> = { [path]: JSON.stringify({ scopes: { 'ItemClasses.Name': { Ring: 'Rings' } } }) };
  const sha = (raw: string) => createHash('sha256').update(raw).digest('hex');
  contents['manifest.json'] = JSON.stringify({ schema_version: 3, files: { [path]: sha(contents[path]) } });
  const fetch = vi.fn(async (url: string) => {
    if (url === '/data/manifest.json') return new Response(JSON.stringify({ version: 'test', files: Object.keys(contents) }));
    const file = url.replace('/data/test/', '');
    if (file === path && state.ready) throw new Error('Do not refetch already verified item class names');
    return new Response(contents[file]);
  });
  vi.stubGlobal('fetch', fetch);
  const backend = await createWasmBackend();
  await backend.init();
  expect(await backend.translateNames([{ kind: 'itemClass', id: 'Ring', name: 'Rings' }], 'zh-CN')).toEqual(['Rings']);
  expect(fetch.mock.calls.filter(([url]) => url.endsWith(path))).toHaveLength(1);
});
