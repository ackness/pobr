import { expect, test } from 'vitest';
import type { ClassNames, GemCatalogEntry, PassiveTreeMeta } from '../api/types';
import { NativeNameIndex, classDisplayName, nativeClassNames, nativeGemNames } from './nativeDisplayNames';
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

test('a class keeps its unique Characters.Name translation when only another table collides on the literal', () => {
  const classes: PassiveTreeMeta = { tree: 'default', classes: [
    { name: 'Shadow', base_str: 0, base_dex: 0, base_int: 0 }, { name: 'Duelist', base_str: 0, base_dex: 0, base_int: 0 }] };
  // Words "Shadow" is 阴影, the class is 暗影, so the literal map holds null.
  const source = catalog({ 'Characters.Name': { 'Metadata/Characters/DexInt/DexIntFour': ['Shadow', '暗影'],
    'Metadata/Characters/StrDex/StrDexFour': ['Duelist', null] } }, 'zh-CN', { Shadow: null, Duelist: null });
  const names = nativeClassNames({ classes: { Shadow: '旧暗影', Duelist: '决斗者' }, ascendancies: {} }, classes, [source]);
  expect(classDisplayName(names, 'Shadow', 'zh-CN', 'classes')).toBe('暗影');
  // Unresolved in the catalog itself: English, never the older dictionary name.
  expect(classDisplayName(names, 'Duelist', 'zh-CN', 'classes')).toBe('Duelist');
});

const passiveCatalog = (passives?: NativeDisplayTexts['scopes'][string], locale: 'zh-CN' | 'zh-TW' = 'zh-CN') =>
  catalog(passives ? { 'PassiveSkills.Name': passives } : {}, locale, { 'Mana Regeneration': null, 'Seeing Stars': '旧译' });
const passive = (id: string, name: string) => ({ kind: 'passive' as const, id, name });

test('passive names resolve by PassiveSkills id, even when one English has several translations', () => {
  const index = new NativeNameIndex([passiveCatalog({
    mana10: ['Mana Regeneration', '魔力再生'], energy_shield_recovery10: ['Mana Regeneration', '魔力回复'],
    seeing_stars: ['Seeing Stars', null], blank: ['Ruin', '  '],
  })]);
  expect(index.resolve(passive('mana10', 'Mana Regeneration'), 'zh-CN')).toBe('魔力再生');
  expect(index.resolve(passive('energy_shield_recovery10', 'Mana Regeneration'), 'zh-CN')).toBe('魔力回复');
  // Explicitly unresolved: English, not the literal map's older translation.
  expect(index.resolve(passive('seeing_stars', 'Seeing Stars'), 'zh-CN')).toBeNull();
  expect(index.resolve(passive('blank', 'Ruin'), 'zh-CN')).toBeNull();
  // Unowned id, or the id now shows other English (a transformed node, stale data): the text path decides.
  expect(index.resolve(passive('mana_regeneration5', 'Mana Regeneration'), 'zh-CN')).toBeUndefined();
  expect(index.resolve(passive('mana10', 'Devoted Protector'), 'zh-CN')).toBeUndefined();
  // Never another locale's catalog.
  expect(index.resolve(passive('mana10', 'Mana Regeneration'), 'zh-TW')).toBeUndefined();
  expect(index.resolve(passive('mana10', 'Mana Regeneration'), 'en-US')).toBeUndefined();

  const { pending, merge } = index.split([passive('mana10', 'Mana Regeneration'), passive('x', 'Ruin'),
    passive('seeing_stars', 'Seeing Stars'), passive('y', 'Other')], 'zh-CN');
  expect(pending).toEqual(['Ruin', 'Other']);
  expect(merge(['废墟', 'Other'])).toEqual(['魔力再生', '废墟', 'Seeing Stars', 'Other']);
});

test('without a PassiveSkills.Name scope every passive name keeps the current text path', () => {
  const requests = [passive('mana10', 'Mana Regeneration'), passive('s', 'Seeing Stars')];
  for (const catalogs of [[passiveCatalog()], [], [passiveCatalog({ mana10: ['Mana Regeneration', '魔力再生'] }, 'zh-TW')]]) {
    const { pending, merge } = new NativeNameIndex(catalogs).split(requests, 'zh-CN');
    expect(pending).toEqual(['Mana Regeneration', 'Seeing Stars']);
    expect(merge(['Mana Regeneration', '旧译'])).toEqual(['Mana Regeneration', '旧译']);
  }
});

test('the text path gets each repeated name once and every node gets its translation back', () => {
  const requests = [passive('a', 'Life'), passive('b', 'Mana'), passive('c', 'Life'), passive('d', 'Life')];
  const { pending, merge } = new NativeNameIndex([]).split(requests, 'zh-CN');
  expect(pending).toEqual(['Life', 'Mana']);
  expect(merge(['生命', '魔力'])).toEqual(['生命', '魔力', '生命', '生命']);
});

test('base lines resolve through every BaseItemTypes id with that English, never by order', () => {
  const bases = [{ id: 'Metadata/Items/Jewels/JewelInt', name: 'Sapphire' }, { id: 'A1', name: 'Twin' }, { id: 'A2', name: 'Twin' },
    { id: 'B1', name: 'Agreed' }, { id: 'B2', name: 'Agreed' }, { id: 'U', name: 'Unowned' }];
  // "Sapphire" is also a Mods.Name affix (蓝玉的), so its literal entry is null.
  const source = catalog({ 'BaseItemTypes.Name': { 'Metadata/Items/Jewels/JewelInt': ['Sapphire', '蓝玉'],
    A1: ['Twin', '甲'], A2: ['Twin', '乙'], B1: ['Agreed', '同'], B2: ['Agreed', '同'] } }, 'zh-CN', { Sapphire: null });
  const index = new NativeNameIndex([source], bases);
  const base = (name: string) => ({ kind: 'base' as const, name });
  expect(index.resolve(base('Sapphire'), 'zh-CN')).toBe('蓝玉');
  expect(index.resolve(base('Twin'), 'zh-CN')).toBeNull();
  expect(index.resolve(base('Agreed'), 'zh-CN')).toBe('同');
  expect(index.resolve(base('Unowned'), 'zh-CN')).toBeUndefined();
  expect(index.resolve(base('Not a base'), 'zh-CN')).toBeUndefined();
  expect(index.resolve(base('Sapphire'), 'zh-TW')).toBeUndefined();
  expect(new NativeNameIndex([catalog({})], bases).resolve(base('Sapphire'), 'zh-CN')).toBeUndefined();
});

const monster = (id: string | undefined, name: string) => ({ kind: 'monster' as const, id, name });

test('minion names come only from MonsterVarieties.Name by variety id, mixing official and supplement records', () => {
  const source = { ...catalog({ 'MonsterVarieties.Name': {
    'Metadata/Monsters/A': ['Raging Spirit', '愤怒之灵'], 'Metadata/Monsters/B': ['Skeletal Warrior', '骷髅战士'],
    'Metadata/Monsters/C': ['Unresolved', null],
  } }), origins: { entries: {}, scopes: { 'MonsterVarieties.Name': { 'Metadata/Monsters/B': 'agent' as const } } } };
  const index = new NativeNameIndex([source]);
  expect(index.resolveDetailed(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-CN')).toEqual({ text: '愤怒之灵', origin: null });
  expect(index.resolveDetailed(monster('Metadata/Monsters/B', 'Skeletal Warrior'), 'zh-CN')).toEqual({ text: '骷髅战士', origin: 'agent' });
  // No record, no variety id, other English or no scope: English, never the text path.
  for (const request of [monster('Metadata/Monsters/C', 'Unresolved'), monster('Metadata/Monsters/Z', 'Zombie'),
    monster(undefined, 'Raging Spirit'), monster('Metadata/Monsters/A', 'Renamed')]) {
    expect(index.resolveDetailed(request, 'zh-CN')).toBeNull();
  }
  expect(new NativeNameIndex([catalog({})]).resolve(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-CN')).toBeNull();
  expect(new NativeNameIndex([]).resolve(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-CN')).toBeNull();
  const { pending, mergeDetailed } = index.split([monster('Metadata/Monsters/B', 'Skeletal Warrior'),
    monster('Metadata/Monsters/Z', 'Zombie'), passive('p', 'Life')], 'zh-CN');
  expect(pending).toEqual(['Life']);
  expect(mergeDetailed([{ text: '生命', origin: 'human' }])).toEqual([
    { text: '骷髅战士', origin: 'agent' }, { text: 'Zombie', origin: null }, { text: '生命', origin: 'human' }]);
});

test('passive and base names carry their scope record origin; absent origins are official', () => {
  const scopes = { 'PassiveSkills.Name': { mana10: ['Mana Regeneration', '魔力再生'] as [string, string] },
    'BaseItemTypes.Name': { B1: ['Agreed', '同'] as [string, string], B2: ['Agreed', '同'] as [string, string] } };
  const bases = [{ id: 'B1', name: 'Agreed' }, { id: 'B2', name: 'Agreed' }];
  const official = new NativeNameIndex([catalog(scopes)], bases);
  expect(official.resolveDetailed(passive('mana10', 'Mana Regeneration'), 'zh-CN')).toEqual({ text: '魔力再生', origin: null });
  const ours = new NativeNameIndex([{ ...catalog(scopes), origins: { entries: {}, scopes: {
    'PassiveSkills.Name': { mana10: 'human' as const }, 'BaseItemTypes.Name': { B2: 'agent' as const } } } }], bases);
  expect(ours.resolveDetailed(passive('mana10', 'Mana Regeneration'), 'zh-CN')).toEqual({ text: '魔力再生', origin: 'human' });
  expect(ours.resolveDetailed({ kind: 'base', name: 'Agreed' }, 'zh-CN')).toEqual({ text: '同', origin: 'agent' });
  expect(ours.resolve({ kind: 'base', name: 'Agreed' }, 'zh-CN')).toBe('同');
});

test('zh-TW minion names convert the zh-CN record unless a zh-TW catalog names the variety itself', () => {
  const cn = { ...catalog({ 'MonsterVarieties.Name': {
    'Metadata/Monsters/A': ['Raging Spirit', '愤怒之灵'], 'Metadata/Monsters/B': ['Skeletal Warrior', '骷髅战士'],
    'Metadata/Monsters/C': ['Skeletal Brute', '骷髅蛮兵'],
  } }), origins: { entries: {}, scopes: { 'MonsterVarieties.Name': { 'Metadata/Monsters/B': 'agent' as const } } } };
  const cnOnly = new NativeNameIndex([cn]);
  // A converted supplement is still a supplement.
  expect(cnOnly.resolveDetailed(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-TW')).toEqual({ text: '憤怒之靈', origin: null });
  expect(cnOnly.resolveDetailed(monster('Metadata/Monsters/B', 'Skeletal Warrior'), 'zh-TW')).toEqual({ text: '骷髏戰士', origin: 'agent' });
  expect(cnOnly.resolveDetailed(monster('Metadata/Monsters/A', 'Renamed'), 'zh-TW')).toBeNull();
  expect(cnOnly.resolveDetailed(monster(undefined, 'Raging Spirit'), 'zh-TW')).toBeNull();
  // A zh-TW record wins as written; its explicit null keeps English; an id it lacks falls back to zh-CN.
  const tw = catalog({ 'MonsterVarieties.Name': {
    'Metadata/Monsters/A': ['Raging Spirit', '狂怒之靈'], 'Metadata/Monsters/B': ['Skeletal Warrior', null],
  } }, 'zh-TW');
  const both = new NativeNameIndex([cn, tw]);
  expect(both.resolveDetailed(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-TW')).toEqual({ text: '狂怒之靈', origin: null });
  expect(both.resolveDetailed(monster('Metadata/Monsters/B', 'Skeletal Warrior'), 'zh-TW')).toBeNull();
  expect(both.resolveDetailed(monster('Metadata/Monsters/C', 'Skeletal Brute'), 'zh-TW')).toEqual({ text: '骷髏蠻兵', origin: null });
  // zh-CN keeps its own record and script.
  expect(both.resolveDetailed(monster('Metadata/Monsters/A', 'Raging Spirit'), 'zh-CN')).toEqual({ text: '愤怒之灵', origin: null });
});

test('a supplement scope record yields to an official text-path translation, never the reverse', () => {
  const source = { ...catalog({ 'PassiveSkills.Name': {
    a: ['Mana Regeneration', '我方魔力再生'], b: ['Life', '我方生命'], c: ['Official Node', '官方节点'],
  } }), origins: { entries: {}, scopes: { 'PassiveSkills.Name': { a: 'agent' as const, b: 'human' as const } } } };
  const index = new NativeNameIndex([source]);
  const { pending, mergeDetailed } = index.split([passive('a', 'Mana Regeneration'), passive('b', 'Life'),
    passive('c', 'Official Node'), passive('a', 'Mana Regeneration')], 'zh-CN');
  // Supplement records also ask the text path; official ones do not.
  expect(pending).toEqual(['Mana Regeneration', 'Life']);
  expect(mergeDetailed([{ text: '魔力再生', origin: null }, { text: 'Life', origin: null }])).toEqual([
    { text: '魔力再生', origin: null }, // official text path wins
    { text: '我方生命', origin: 'human' }, // the text path left it English
    { text: '官方节点', origin: null },
    { text: '魔力再生', origin: null },
  ]);
  // A supplement text path never replaces a supplement record.
  expect(mergeDetailed([{ text: '另一补译', origin: 'agent' }, { text: '生命', origin: 'agent' }]).slice(0, 2))
    .toEqual([{ text: '我方魔力再生', origin: 'agent' }, { text: '我方生命', origin: 'human' }]);
  // A minion's supplement record stays: minions have no text path.
  const minions = new NativeNameIndex([{ ...catalog({ 'MonsterVarieties.Name': { m: ['Wolf', '狼'] } }),
    origins: { entries: {}, scopes: { 'MonsterVarieties.Name': { m: 'agent' as const } } } }]);
  const split = minions.split([monster('m', 'Wolf')], 'zh-CN');
  expect(split.pending).toEqual([]);
  expect(split.mergeDetailed([])).toEqual([{ text: '狼', origin: 'agent' }]);
});
