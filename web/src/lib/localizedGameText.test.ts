import { expect, test } from 'vitest';
import type { GemCatalogEntry } from '../api/types';
import { GameTextLocalizer } from './localizedGameText';
import { gemDisplayName } from './skillNames';
import { localizeStructLine, cleanLine, itemLines } from '../hooks/useLocalizedLines';
import gems from '../fixtures/gem_catalog.json';

const localizer = new GameTextLocalizer(gems);

test('display lines use native requested names and convert only missing translations', () => {
  const originals = ['Wind Dancer', 'Grants Level 20 Wind Dancer Skill', '+50 to maximum Life', 'Unrecognized debug: raw_ID'];
  const cn = ['风舞者', '获得 20 级风舞者技能', '生命上限 +50', 'Unrecognized debug: raw_ID'];
  expect(localizer.localizeLines(originals, cn, 'en-US')).toEqual(originals);
  expect(localizer.localizeLines(originals, cn, 'zh-CN')).toEqual(cn);
  expect(localizer.localizeLines(originals, cn, 'zh-TW')).toEqual([
    '風魔舞者', '獲得 20 級風魔舞者技能', '生命上限 +50', 'Unrecognized debug: raw_ID',
  ]);
  expect(originals[0]).toBe('Wind Dancer');
  expect(cn[0]).toBe('风舞者');
});

test('generic translated prose is not rewritten just because it contains a gem name', () => {
  const names = new GameTextLocalizer([{ name: 'Wind Dancer', name_zh_cn: '风舞者', name_zh_tw: '風魔舞者' }]);
  expect(names.localize('A different source', '风舞者测试', 'zh-TW')).toBe('風舞者測試');
  expect(names.localize('Wind Dancers unknown plural', '风舞者测试', 'zh-TW')).toBe('風舞者測試');
});

test('missing language data converts the alternate script without claiming a native name', () => {
  const gem = { ...(gems[0] as GemCatalogEntry), name: 'Example', name_zh_cn: '能量护盾', name_zh_tw: '' };
  expect(gemDisplayName(gem, 'zh-TW')).toBe('能量護盾');
  expect(gemDisplayName({ ...gem, name_zh_cn: '', name_zh_tw: '圖騰護甲' }, 'zh-CN')).toBe('图腾护甲');
  expect(gemDisplayName({ ...gem, name_zh_cn: '', name_zh_tw: '' }, 'zh-CN')).toBe('Example');
  expect(new GameTextLocalizer([]).localize('Untranslated', '伤害提高 20%', 'zh-TW')).toBe('傷害提高 20%');
});

test('ambiguous alternate names do not choose another skill arbitrarily', () => {
  const names = new GameTextLocalizer([
    { name: 'First', name_zh_cn: '同名', name_zh_tw: '第一個' },
    { name: 'Second', name_zh_cn: '同名', name_zh_tw: '第二個' },
  ]);
  expect(names.localize('同名', '同名', 'zh-TW')).toBe('同名');
  expect(names.localize('Second', '同名', 'zh-TW')).toBe('第二個');
  expect(names.localize('Grants Second', '获得同名', 'zh-TW')).toBe('獲得第二個');
});

test('structural item display text uses the selected locale and preserves raw item text', () => {
  expect(localizeStructLine('Attacks per Second: 1.25', 'zh-TW')).toBe('每秒攻擊次數: 1.25');
  expect(localizeStructLine('Energy Shield: 500', 'zh-CN')).toBe('能量护盾: 500');
  expect(localizeStructLine('Radius: Large', 'zh-TW')).toBe('範圍: 大型');
  expect(localizeStructLine('Corrupted', 'zh-CN')).toBe('已腐化');
  expect(localizeStructLine('Unknown: raw_ID', 'zh-TW')).toBeNull();
  const raw = 'Rarity: RARE\nExample\nEnergy Shield: 500\n{crafted}+10 to [MaximumLife|maximum Life]';
  expect(itemLines(raw)).toEqual(['Example', 'Energy Shield: 500', '+10 to maximum Life']);
  expect(cleanLine('{crafted}[Strength]')).toBe('Strength');
  expect(raw).toContain('{crafted}');
});
