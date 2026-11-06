import { afterEach, expect, it, vi } from 'vitest';
import { buildGemDescriptionIndex, gemDescription, gemDescriptionScopes, loadGemDescriptionIndex, renderGemDescription } from './gemDescriptions';
import { nativeDisplayTexts, type NativeDisplayTexts } from './nativeDisplayTexts';
import { setGameLocales } from './gameLocales';

const support = { skill_id: 'SupportEffect', is_support: true };
const active = { skill_id: 'ActiveEffect', is_support: false };
const raw = 'Supports [Area|area] Skills.\r\n';
const links = [
  { gem_id: 'GemOne', granted_effect_id: support.skill_id, variant_id: 'VariantOne' },
  { gem_id: 'GemTwo', granted_effect_id: support.skill_id, variant_id: 'VariantTwo' },
];
const file = (supports: Record<string, string> = { VariantOne: raw, VariantTwo: raw }, active: Record<string, string> = { active_id: 'Cast a [Spell].' }) =>
  ({ scopes: { 'GemEffects.SupportText': supports, 'ActiveSkills.Description': active } });
function catalog(locale: 'zh-CN' | 'zh-TW', records: Record<string, [string | null, string | null]>, scope = 'GemEffects.SupportText'): NativeDisplayTexts {
  return nativeDisplayTexts({ schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale,
    catalog_sha256: 'a'.repeat(64), entries: {}, scopes: { [scope]: records } }, 'test', locale);
}
const pair = (text: string | null, english = raw): Record<string, [string, string | null]> =>
  ({ VariantOne: [english, text], VariantTwo: [english, text] });
const index = (catalogs: NativeDisplayTexts[] = [], descriptions = file()) =>
  buildGemDescriptionIndex(descriptions, links, { ActiveEffect: 'active_id' }, catalogs, []);
const english = { text: 'Supports area Skills.', lang: 'en', english: true };
afterEach(() => setGameLocales([]));

it.each([
  ['[A|B]', 'B'], ['[A|]', ''], ['[A]', 'A'], ['[A] [B|C]', 'A C'],
  ['First\r\nSecond', 'First\nSecond'], ['First\\nSecond', 'First\nSecond'],
  ['First\\r\\nSecond', 'First\nSecond'], [' \t [Key| Text ] \r\n', 'Text'],
  // A third part is not game markup: the brackets stay, so the text is rejected rather than shown as "B|C".
  ['[A|B|C]', '[A|B|C]'],
])('renders %j as %j', (raw, expected) => expect(renderGemDescription(raw)).toBe(expected));

it('uses support variant IDs and requires agreement of all English-bearing variants, regardless of order', () => {
  const data = index();
  expect(gemDescription(data, support, 'en-US')).toEqual(english);
  expect(gemDescription(index([], file({ VariantOne: raw })), support, 'en-US')).toEqual(english);
  expect(gemDescription(index([], file({ VariantTwo: raw })), support, 'en-US')).toEqual(english);
  for (const order of [links, [...links].reverse()]) {
    const different = buildGemDescriptionIndex(file({ VariantOne: raw, VariantTwo: 'Another text' }), order, {}, [], []);
    expect(gemDescription(different, support, 'en-US')).toBeNull();
  }
  expect(gemDescription(index([], file({ SupportEffect: raw })), support, 'en-US')).toBeNull();
});

it('finds active, spirit and meta descriptions through granted effect -> active_skill', () => {
  const data = index([catalog('zh-CN', { active_id: ['Cast a [Spell].', '施放[Spell|法术]。'] }, 'ActiveSkills.Description')]);
  expect(gemDescription(data, active, 'en-US')).toEqual({ text: 'Cast a Spell.', lang: 'en', english: true });
  expect(gemDescription(data, active, 'zh-CN')).toEqual({ text: '施放法术。', lang: 'zh-CN', english: false });
  expect(gemDescription(data, { ...active, skill_id: 'active_id' }, 'zh-CN')).toBeNull();
});

it('English never uses a translation; zh-CN uses its own exact-source record', () => {
  const data = index([catalog('zh-CN', pair('辅助[Area|范围]技能。'))]);
  expect(gemDescription(data, support, 'en-US')).toEqual(english);
  expect(gemDescription(data, support, 'zh-CN')).toEqual({ text: '辅助范围技能。', lang: 'zh-CN', english: false });
});

it('zh-TW uses its own catalog, then zh-CN converted to Traditional, then English', () => {
  const cn = catalog('zh-CN', pair('辅助[Area|范围]技能。'));
  expect(gemDescription(index([cn, catalog('zh-TW', pair('輔助[Area|範圍]技能！'))]), support, 'zh-TW'))
    .toEqual({ text: '輔助範圍技能！', lang: 'zh-TW', english: false });
  expect(gemDescription(index([cn, catalog('zh-TW', {})]), support, 'zh-TW'))
    .toEqual({ text: '輔助範圍技能。', lang: 'zh-TW', english: false });
  expect(gemDescription(index(), support, 'zh-TW')).toEqual(english);
});

it.each(['old English', raw.trim(), null])('skips stale/non-byte-identical English %j and follows the next catalog', source => {
  const tw = catalog('zh-TW', { VariantOne: [source, '舊文'], VariantTwo: [source, '舊文'] });
  expect(gemDescription(index([tw, catalog('zh-CN', pair('范围'))]), support, 'zh-TW'))
    .toEqual({ text: '範圍', lang: 'zh-TW', english: false });
});

it.each([null, '', ' \r\n\t', '[A|]'])('an explicit null/blank translation %j stops the chain at English', text => {
  const data = index([catalog('zh-TW', pair(text)), catalog('zh-CN', pair('范围'))]);
  expect(gemDescription(data, support, 'zh-TW')).toEqual(english);
});

it('missing English members never acquire text from a language catalog', () => {
  const data = index([catalog('zh-CN', pair('范围'))], file({}, {}));
  expect(gemDescription(data, support, 'zh-CN')).toBeNull();
  expect(gemDescription(data, active, 'zh-CN')).toBeNull();
});

it.each(['[Broken', 'Broken]', '[outer [inner]]', '[Projectile|Projectiles|投射物]'])('skips a translation with leftover brackets: %s', text => {
  const data = index([catalog('zh-TW', pair(text)), catalog('zh-CN', pair('范围'))]);
  expect(gemDescription(data, support, 'zh-TW')).toEqual({ text: '範圍', lang: 'zh-TW', english: false });
  expect(gemDescription(index([catalog('zh-CN', pair(text))]), support, 'zh-CN')).toEqual(english);
});

it.each(['[Broken', 'Broken]', '[outer [inner]]', '[A|]'])('hides an unusable rendered English description: %s', text => {
  expect(gemDescription(index([], file({ VariantOne: text })), support, 'en-US')).toBeNull();
});

it('disagreeing resolved variant translations show the agreed English, including missing/explicit fallback variants', () => {
  for (const records of [
    { VariantOne: [raw, '范围'], VariantTwo: [raw, '区域'] },
    { VariantOne: [raw, '范围'] },
    { VariantOne: [raw, '范围'], VariantTwo: [raw, null] },
  ] as Array<Record<string, [string, string | null]>>) {
    expect(gemDescription(index([catalog('zh-CN', records)]), support, 'zh-CN')).toEqual(english);
  }
});

it('variants can agree after each independently resolves through the locale chain', () => {
  const data = index([catalog('zh-TW', { VariantOne: [raw, '範圍'] }), catalog('zh-CN', { VariantTwo: [raw, '范围'] })]);
  expect(gemDescription(data, support, 'zh-TW')).toEqual({ text: '範圍', lang: 'zh-TW', english: false });
});

it('uses gameTextChain, including pack-declared fallback order', () => {
  setGameLocales([{ code: 'zh-CN', fallback: ['zh-TW'], chain: ['zh-CN', 'zh-TW'], templates: false }]);
  expect(gemDescription(index([catalog('zh-TW', pair('範圍'))]), support, 'zh-CN'))
    .toEqual({ text: '范围', lang: 'zh-CN', english: false });
});

it.each([null, [], {}, { scopes: {} }, { scopes: { 'ActiveSkills.Description': {} } },
  { scopes: { ...file().scopes, Extra: {} } }, file({ bad: '' }), file({ bad: '  ' }), file({ bad: null! }),
  { scopes: { 'GemEffects.SupportText': [], 'ActiveSkills.Description': {} } }, file({}, { bad: 42 as unknown as string }),
])('rejects invalid description file shape: %j', value => expect(() => gemDescriptionScopes(value)).toThrow(/shape/));

it('accepts exactly two scopes, ignoring audit metadata', () => {
  expect(gemDescriptionScopes({ _meta: { anything: true }, ...file({}, {}) })).toEqual(file({}, {}).scopes);
});

it('requirements use the name links and overlay overrides, requiring every linked gem to agree', () => {
  const gems = [
    { id: 'GemOne', granted_effect_id: 'Old', str_pct: 50, dex_pct: 0, int_pct: 50 },
    { id: 'GemTwo', granted_effect_id: 'Other', str_pct: 25, dex_pct: 0, int_pct: 75 },
    { id: 'ActiveGem', granted_effect_id: 'ActiveEffect', dex_pct: 100 },
  ];
  const data = buildGemDescriptionIndex(file(), links, {}, [], gems);
  expect(data.requirements.SupportEffect).toEqual(['str', 'int']);
  expect(data.requirements.ActiveEffect).toEqual(['dex']);
  expect(data.requirements.Old).toBeUndefined();
  expect(data.requirements.Unknown).toBeUndefined();
  expect(buildGemDescriptionIndex(file(), links, {}, [], [...gems, { id: 'Third', granted_effect_id: 'SupportEffect', dex_pct: 100 }])
    .requirements.SupportEffect).toBeNull();
});

it('index stays cloneable across the Worker boundary without mutating its inputs', () => {
  const descriptions = file();
  const snapshot = structuredClone(descriptions);
  const data = index([catalog('zh-CN', pair('范围'))], descriptions);
  expect(gemDescription(structuredClone(data), support, 'zh-CN')).toEqual(gemDescription(data, support, 'zh-CN'));
  expect(descriptions).toEqual(snapshot);
});

it('does not read any dependent files or catalogs when the description file is unlisted', async () => {
  const read = vi.fn(), catalogs = vi.fn();
  expect(await loadGemDescriptionIndex(['base/granted_effects.json'], read, catalogs)).toBeNull();
  expect(read).not.toHaveBeenCalled();
  expect(catalogs).not.toHaveBeenCalled();
});
