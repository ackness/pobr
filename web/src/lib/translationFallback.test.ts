import { expect, test } from 'vitest';
import { GameTextLocalizer } from './localizedGameText';
import { nativeDisplayTexts } from './nativeDisplayTexts';
import type { Lang } from './i18n';

// What a player sees for game text the language data lacks (counted by
// `mise run i18n:coverage`): the English text, never an empty or garbled string.
const catalog = nativeDisplayTexts({
  schema: 'pobr-native-display-texts/v1', game_patch: 'test', realm: 'international', locale: 'zh-TW',
  catalog_sha256: 'a'.repeat(64), entries: { 'Native Gap': null }, scopes: {},
}, 'test', 'zh-TW');
const localizer = new GameTextLocalizer([
  { name: 'Untranslated Base' },
  { name: 'Blank Base', name_zh_cn: '', name_zh_tw: '  ' },
  { name: 'Known Base', name_zh_cn: '已知基底', name_zh_tw: '已知基底' },
], [catalog]);

test.each<Lang>(['zh-CN', 'zh-TW'])('missing game text falls back to English in %s', lang => {
  // A name neither locale translates, or translates as blank: the English name.
  expect(localizer.localize('Untranslated Base', 'Untranslated Base', lang)).toBe('Untranslated Base');
  expect(localizer.localize('Blank Base', 'Blank Base', lang)).toBe('Blank Base');
  // A line the engine had no template for comes back in English and stays English.
  expect(localizer.localize('+5 to an Unknown Stat', '+5 to an Unknown Stat', lang)).toBe('+5 to an Unknown Stat');
  // A native display row marked missing shows the English text, not an older translation.
  expect(localizer.localize('Native Gap', 'Native Gap', lang)).toBe('Native Gap');
  // Known names inside an otherwise untranslated line are still shown in the language.
  expect(localizer.localize('Grants Known Base', 'Grants Known Base', lang)).toBe('Grants 已知基底');
});
