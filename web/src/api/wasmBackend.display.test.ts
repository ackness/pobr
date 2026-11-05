import { afterEach, expect, test, vi } from 'vitest';
import { EXPECTED_SCHEMA_VERSION } from './types';
import { createWasmBackend } from './wasmBackend';
import type { GemCatalogEntry } from './types';
import { classDisplayName } from '../lib/nativeDisplayNames';
import { gemDisplayName } from '../lib/skillNames';

const state = vi.hoisted(() => ({ gems: [] as GemCatalogEntry[], crafting: { bases: [] as unknown[], mods: [] as unknown[] } }));

// Match the runtime import URL even when no local WASM package exists.
vi.mock('/src/wasm/pkg/pobr_wasm.js', () => ({
  default: async () => {}, schemaVersion: () => EXPECTED_SCHEMA_VERSION,
  gemCatalogJson: () => JSON.stringify(state.gems),
  itemCraftingCatalogJson: () => JSON.stringify(state.crafting),
  isDataReady: () => true,
  availableLocales: () => JSON.stringify([
    { code: 'zh-CN', fallback: [], chain: ['zh-CN'], templates: true },
    { code: 'zh-TW', fallback: [], chain: ['zh-TW'], templates: false },
  ]),
  translateLines: (lang: string, raw: string) =>
    JSON.stringify((JSON.parse(raw) as string[]).map(() => lang === 'zh-CN' ? '旧译文' : 'wrong locale')),
}));

afterEach(() => { vi.unstubAllGlobals(); state.gems = []; state.crafting = { bases: [], mods: [] }; });

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
    active_effect_ids: ['Effect'], colour: 'int', is_support: false, is_lineage: false, tags: ['Spell'] }];
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
