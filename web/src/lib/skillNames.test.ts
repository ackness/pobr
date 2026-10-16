import { expect, test } from 'vitest';
import type { GemCatalogEntry } from '../api/types';
import { gemDisplayName, skillDisplayName } from './skillNames';

const wind: GemCatalogEntry = { skill_id: 'WindDancerPlayer', name: 'Wind Dancer', name_zh_cn: '风舞者',
  name_zh_tw: '風魔舞者', additional_skill_ids: ['TriggeredWindDancerPlayer'], colour: 'dex',
  active_effect_ids: ['WindDancerPlayer', 'TriggeredWindDancerPlayer'], is_support: false, is_lineage: false, tags: [] };
const catalog = new Map([[wind.skill_id, wind]]);

test('secondary trigger names reuse localized gem names without rewriting IDs', () => {
  expect(skillDisplayName('TriggeredWindDancerPlayer', 'zh-CN', catalog)).toBe('风舞者（触发）');
  expect(skillDisplayName('TriggeredWindDancerPlayer', 'zh-TW', catalog)).toBe('風魔舞者（觸發）');
  expect(skillDisplayName('TriggeredWindDancerPlayer', 'en-US', catalog)).toBe('Triggered Wind Dancer');
  expect(skillDisplayName('WindDancerPlayer', 'zh-CN', catalog)).toBe('风舞者');
  expect(catalog.size).toBe(1);
});

test('exact skill names win over aliases and unknown IDs remain identifiable', () => {
  const exact = { ...wind, skill_id: 'TriggeredWindDancerPlayer', name_zh_cn: '独立技能' };
  expect(skillDisplayName(exact.skill_id, 'zh-CN', new Map([...catalog, [exact.skill_id, exact]]))).toBe('独立技能');
  expect(skillDisplayName('UnknownSkillPlayer', 'zh-CN', catalog)).toBe('Unknown Skill');
  expect(skillDisplayName('TriggeredWindDancerPlayer', 'zh-CN', new Map([[wind.skill_id, { ...wind, additional_skill_ids: undefined }]]))).toBe('Triggered Wind Dancer');
});

test('summoner gem names omit the unresolved monster template in each language', () => {
  const summoner = { ...wind, name: 'Companion: {0}', name_zh_cn: '召唤伙伴：{0}', name_zh_tw: '召喚夥伴：{0}' };
  expect(gemDisplayName(summoner, 'en-US')).toBe('Companion');
  expect(gemDisplayName(summoner, 'zh-CN')).toBe('召唤伙伴');
  expect(gemDisplayName(summoner, 'zh-TW')).toBe('召喚夥伴');
  expect(gemDisplayName({ ...summoner, name: 'Spectre: {0}' }, 'en-US')).toBe('Spectre');
  expect(gemDisplayName({ ...summoner, name: 'Command: Charge' }, 'en-US')).toBe('Command: Charge');
});
