import { expect, test } from 'vitest';
import type { GemCatalogEntry } from '../api/types';
import { unmodelledMetaTriggerNote } from './triggerNotes';

const trigger = { meta_gem_id: 'MetaCastWhenStunnedPlayer', host_skill_id: 'CometPlayer' };
const catalog = new Map([[trigger.meta_gem_id, {
  skill_id: trigger.meta_gem_id, name: 'Cast when Stunned', name_zh_cn: '晕眩时施放', name_zh_tw: '暈眩時施放',
} as GemCatalogEntry]]);

test.each([
  ['en-US', 'Triggered by Cast when Stunned: PoBR does not model how often this gem gains Energy, so its DPS is shown as 0. Average hit is still calculated.'],
  ['zh-CN', '由 晕眩时施放 触发：PoBR 尚未建模这颗宝石获取能量的频率，因此 DPS 显示为 0。平均击中伤害仍会计算。'],
  ['zh-TW', '由 暈眩時施放 觸發：PoBR 尚未模擬這顆寶石獲得能量的頻率，因此 DPS 顯示為 0。仍會計算平均擊中傷害。'],
] as const)('explains the zero and preserved average hit with the gem display name in %s', (lang, expected) => {
  expect(unmodelledMetaTriggerNote(trigger, lang, catalog)).toBe(expected);
  expect(unmodelledMetaTriggerNote(trigger, lang, catalog)).not.toContain(trigger.meta_gem_id);
});
