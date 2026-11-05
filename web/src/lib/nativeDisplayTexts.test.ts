import { expect, test } from 'vitest';
import { listedOrigin, nativeDisplayTexts } from './nativeDisplayTexts';
import { GameTextLocalizer } from './localizedGameText';

const data = (locale = 'zh-TW') => ({
  schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale,
  catalog_sha256: 'a'.repeat(64), entries: { Known: '原生', Missing: null, Collision: null },
  scopes: { 'ItemClasses.Name': { id: ['Known', '原生'] } },
});

test('source-owned literal text overrides old prose, while missing or ambiguous rows block stale translations', () => {
  const catalog = nativeDisplayTexts(data(), 'test', 'zh-TW');
  const localizer = new GameTextLocalizer([
    { name: 'Known', name_zh_cn: '旧翻译', name_zh_tw: '舊翻譯' },
    { name: 'Missing', name_zh_cn: '旧值' },
  ], [catalog]);
  expect(localizer.localize('Known', '旧翻译', 'zh-TW')).toBe('原生');
  expect(localizer.localize('Missing', '旧值', 'zh-TW')).toBe('Missing');
  expect(localizer.localize('Collision', '任意一行', 'zh-TW')).toBe('Collision');
  expect(localizer.localize('Known', '旧翻译', 'zh-CN')).toBe('旧翻译');
  expect(localizer.localize('Known', '旧翻译', 'en-US')).toBe('Known');
  expect(localizer.localize('Unowned', '伤害', 'zh-TW')).toBe('傷害');
});

test('a sidecar belongs to one exact snapshot and locale; malformed or duplicate catalogs fail', () => {
  expect(() => nativeDisplayTexts(data(), 'other', 'zh-TW')).toThrow(/identity/);
  expect(() => nativeDisplayTexts(data(), 'test', 'zh-CN')).toThrow(/identity/);
  expect(() => nativeDisplayTexts({ ...data(), entries: { bad: 1 } }, 'test', 'zh-TW')).toThrow(/shape/);
  expect(() => nativeDisplayTexts({ ...data(), scopes: { a: { b: ['short'] } } }, 'test', 'zh-TW')).toThrow(/shape/);
  const catalog = nativeDisplayTexts(data(), 'test', 'zh-TW');
  expect(() => new GameTextLocalizer([], [catalog, catalog])).toThrow(/Duplicate/);
});

test('whole literal descriptions are not substituted or converted across locales', () => {
  const value = { ...data(), entries: { ...data().entries, 'Fire a [Projectile].': '發射一枚 [Projectile|投射物]。' } };
  const localizer = new GameTextLocalizer([], [nativeDisplayTexts(value, 'test', 'zh-TW')]);
  expect(localizer.localize('Fire a [Projectile].', 'Old', 'zh-TW')).toBe('發射一枚 [Projectile|投射物]。');
  expect(localizer.localize('Fire a [Projectile]. Changed', 'Old', 'zh-TW')).toBe('Old');
});

test('supplement origins are optional; a listed literal keeps its origin, every other text is official', () => {
  // Absent: exactly today's catalogs.
  expect(nativeDisplayTexts(data(), 'test', 'zh-TW').origins).toBeUndefined();
  const value = { ...data(), entries: { ...data().entries, Ours: '我方' },
    origins: { entries: { Ours: 'agent' }, scopes: { 'ItemClasses.Name': { id: 'human' } } } };
  const catalog = nativeDisplayTexts(value, 'test', 'zh-TW');
  const localizer = new GameTextLocalizer([], [catalog]);
  expect(localizer.localizeDetailed('Ours', { text: 'Ours', origin: null }, 'zh-TW')).toEqual({ text: '我方', origin: 'agent' });
  // A supplement never overrides official text: an official, translated rendering wins over the literal.
  expect(localizer.localizeDetailed('Ours', { text: '官方', origin: null }, 'zh-TW')).toEqual({ text: '官方', origin: null });
  expect(localizer.localizeDetailed('Ours', { text: '补译', origin: 'human' }, 'zh-TW')).toEqual({ text: '我方', origin: 'agent' });
  expect(localizer.localizeDetailed('Known', { text: 'x', origin: null }, 'zh-TW')).toEqual({ text: '原生', origin: null });
  // Unresolved stays English and official; an unowned line keeps the rendered line's origin.
  expect(localizer.localizeDetailed('Missing', { text: 'x', origin: 'agent' }, 'zh-TW')).toEqual({ text: 'Missing', origin: null });
  expect(localizer.localizeDetailed('Unowned', { text: '伤害', origin: 'agent' }, 'zh-TW')).toEqual({ text: '傷害', origin: 'agent' });
  expect(localizer.localizeDetailed('Ours', { text: 'x', origin: 'agent' }, 'en-US')).toEqual({ text: 'Ours', origin: null });
  expect(localizer.localizeLinesDetailed(['Ours', 'Unowned'], [{ text: 'Ours', origin: null }, { text: '伤害', origin: 'human' }], 'zh-TW'))
    .toEqual([{ text: '我方', origin: 'agent' }, { text: '傷害', origin: 'human' }]);
  expect(localizer.localize('Ours', 'Ours', 'zh-TW')).toBe('我方');
  expect(localizer.localize('Ours', '官方', 'zh-TW')).toBe('官方');
  for (const origins of [[], { entries: [] }, { scopes: { a: [] } }, { scopes: [] }]) {
    expect(() => nativeDisplayTexts({ ...value, origins }, 'test', 'zh-TW')).toThrow(/shape/);
  }
  // An origin value the web does not know keeps the catalog and reads as a supplement, never official.
  const unknown = nativeDisplayTexts({ ...value, origins: { entries: { Ours: 'mine' }, scopes: { a: { b: 1 } } } }, 'test', 'zh-TW');
  expect(listedOrigin(unknown.origins?.entries, 'Ours')).toBe('agent');
  expect(listedOrigin(unknown.origins?.scopes?.a, 'b')).toBe('agent');
  expect(listedOrigin({ x: 'official', y: 'human' }, 'x')).toBeNull();
  expect(listedOrigin({ x: 'official', y: 'human' }, 'y')).toBe('human');
});
