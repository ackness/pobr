import { expect, test } from 'vitest';
import type { ClassNames, GemCatalogEntry, PassiveTreeMeta } from '../api/types';
import { classDisplayName, nativeClassNames, nativeGemNames } from './nativeDisplayNames';
import type { NativeDisplayTexts } from './nativeDisplayTexts';
import { gemDisplayName, skillDisplayName } from './skillNames';

const gem: GemCatalogEntry = { skill_id: 'Effect', name: 'Fire', name_zh_cn: '旧火焰', name_zh_tw: '舊火焰',
  additional_skill_ids: ['TriggeredFirePlayer'], active_effect_ids: ['Effect', 'TriggeredFirePlayer'],
  colour: 'int', is_support: false, is_lineage: false, tags: ['Spell'] };
const catalog = (scopes: NativeDisplayTexts['scopes'], locale: 'zh-CN' | 'zh-TW' = 'zh-CN',
  entries: NativeDisplayTexts['entries'] = {}): NativeDisplayTexts => ({
  schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale,
  catalog_sha256: 'a'.repeat(64), scopes, entries,
});
const link = { id: 'Gem/Fire', granted_effect_id: 'Effect' };

test('gem names follow stable links, overlay membership and current English, including secondary display aliases', () => {
  const source = catalog({ 'BaseItemTypes.Name': { 'Gem/Fire': ['Fire', '烈火'], 'Item/Fire': ['Fire', '别的物品'] } });
  const entries = nativeGemNames([gem], [{ ...link, granted_effect_id: 'OldEffect' }],
    [{ gem_id: link.id, granted_effect_id: 'Effect' }], [source]);
  expect(gemDisplayName(entries[0], 'zh-CN')).toBe('烈火');
  expect(gemDisplayName(entries[0], 'zh-TW')).toBe('舊火焰');
  expect(gemDisplayName(entries[0], 'en-US')).toBe('Fire');
  expect(skillDisplayName('TriggeredFirePlayer', 'zh-CN', new Map([['Effect', entries[0]]]))).toBe('烈火（触发）');
  expect(entries[0]).toEqual({ ...gem, name_zh_cn: '烈火' });
  expect(gem.name_zh_cn).toBe('旧火焰');
  expect(nativeGemNames([gem], [link], [{ gem_id: link.id, granted_effect_id: 'Other' }], [source])).toEqual([gem]);
});

test('source English changes and explicit rejections block old and alternate-script names', () => {
  for (const pair of [['Other', '其他'], ['Fire', null], [null, null]] as Array<[string | null, string | null]>) {
    const result = nativeGemNames([gem], [link], [], [catalog({ 'BaseItemTypes.Name': { [link.id]: pair } })])[0];
    expect(gemDisplayName(result, 'zh-CN')).toBe('Fire');
    expect(result.name_zh_tw).toBe(gem.name_zh_tw);
  }
});

test('ambiguous or incomplete gem membership never selects the first translation', () => {
  const links = [link, { id: 'Gem/Other', granted_effect_id: 'Effect' }];
  for (const other of [undefined, ['Fire', '异译'], ['Fire', null]] as Array<[string, string | null] | undefined>) {
    const scope: NativeDisplayTexts['scopes'][string] = { [link.id]: ['Fire', '烈火'] };
    if (other) scope['Gem/Other'] = other;
    const source = catalog({ 'BaseItemTypes.Name': scope });
    const result = nativeGemNames([gem], links, [], [source]);
    expect(result[0].name_zh_cn).toBe('Fire');
    expect(nativeGemNames([gem], [...links].reverse(), [], [source])).toEqual(result);
  }
  const source = catalog({ 'BaseItemTypes.Name': { [link.id]: ['Fire', '烈火'], 'Gem/Other': ['Fire', '烈火'] } });
  expect(nativeGemNames([gem], links, [], [source])[0].name_zh_cn).toBe('烈火');
  source.scopes['BaseItemTypes.Name']['Gem/Other'] = ['Different name', '异名'];
  expect(nativeGemNames([gem], links, [], [source])[0].name_zh_cn).toBe('烈火');
});

test('unowned gem IDs and absent locale catalogs retain the existing compatibility path', () => {
  expect(nativeGemNames([gem], [link], [], [])).toEqual([gem]);
  expect(nativeGemNames([gem], [link], [], [catalog({ 'BaseItemTypes.Name': { unrelated: ['Fire', '其他'] } })])).toEqual([gem]);
});

const legacy: ClassNames = { classes: { Warrior: '旧战士' }, ascendancies: { Titan: '旧泰坦' } };
const meta: PassiveTreeMeta = { tree: 'default', classes: [{ name: 'Warrior', base_str: 10, base_dex: 5, base_int: 5,
  ascendancies: [{ id: 'Warrior1', name: 'Titan' }] }] };

test('class labels use their own scope and ascendancy IDs, independently for each locale', () => {
  const names = nativeClassNames(legacy, meta, [
    catalog({ 'Characters.Name': { WarriorId: ['Warrior', '战士'] }, 'Ascendancy.Name': { Warrior1: ['Titan', '泰坦'] } }),
    catalog({ 'Characters.Name': { WarriorId: ['Warrior', '戰士'] }, 'Ascendancy.Name': { Warrior1: ['Titan', '泰坦王'] } }, 'zh-TW'),
  ]);
  expect(classDisplayName(names, 'Warrior', 'zh-CN', 'classes')).toBe('战士');
  expect(classDisplayName(names, 'Warrior', 'zh-TW')).toBe('戰士');
  expect(classDisplayName(names, 'Titan', 'zh-CN')).toBe('泰坦');
  expect(classDisplayName(names, 'Titan', 'zh-TW', 'ascendancies')).toBe('泰坦王');
  expect(classDisplayName(names, 'Titan', 'en-US')).toBe('Titan');
  expect(classDisplayName(names, 'User set', 'zh-CN')).toBe('User set');
  expect(legacy).toEqual({ classes: { Warrior: '旧战士' }, ascendancies: { Titan: '旧泰坦' } });
});

test('rejected or duplicate class candidates and changed ascendancy English block compatibility', () => {
  const source = catalog({ 'Characters.Name': { W1: ['Warrior', '战士'], W2: [null, null] },
    'Ascendancy.Name': { Warrior1: ['Renamed Titan', '新名'] } }, 'zh-CN', { Warrior: null });
  const names = nativeClassNames(legacy, meta, [source]);
  expect(classDisplayName(names, 'Warrior', 'zh-CN')).toBe('Warrior');
  expect(classDisplayName(names, 'Titan', 'zh-CN')).toBe('Titan');
  source.entries = {};
  source.scopes['Characters.Name'].W2 = ['Warrior', '其他'];
  expect(classDisplayName(nativeClassNames(legacy, meta, [source]), 'Warrior', 'zh-CN')).toBe('Warrior');
});

test('exact ascendancy IDs survive unrelated literal collisions but conflicting target labels remain unresolved', () => {
  const source = catalog({ 'Ascendancy.Name': { Warrior1: ['Titan', '泰坦'], Warrior2: ['Titan', '异译'] } }, 'zh-CN', { Titan: null });
  expect(classDisplayName(nativeClassNames(legacy, meta, [source]), 'Titan', 'zh-CN')).toBe('泰坦');
  const duplicate = structuredClone(meta);
  duplicate.classes[0].ascendancies!.push({ id: 'Warrior2', name: 'Titan' });
  expect(classDisplayName(nativeClassNames(legacy, duplicate, [source]), 'Titan', 'zh-CN')).toBe('Titan');
  expect(classDisplayName(nativeClassNames(legacy, meta, []), 'Titan', 'zh-CN')).toBe('旧泰坦');
});
