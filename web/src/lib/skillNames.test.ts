import { expect, test } from 'vitest';
import type { GemCatalogEntry } from '../api/types';
import { gemDisplayName, skillDisplayName, skillFormName } from './skillNames';

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
  expect(skillDisplayName('UnknownSkillPlayer', 'zh-CN', catalog)).toBe('Unknown Skill（未翻译）');
  expect(skillDisplayName('TriggeredWindDancerPlayer', 'zh-CN', new Map([[wind.skill_id, { ...wind, additional_skill_ids: undefined }]]))).toBe('Triggered Wind Dancer（未翻译）');
});

test('summoner gem names omit the unresolved monster template in each language', () => {
  const summoner = { ...wind, name: 'Companion: {0}', name_zh_cn: '召唤伙伴：{0}', name_zh_tw: '召喚夥伴：{0}' };
  expect(gemDisplayName(summoner, 'en-US')).toBe('Companion');
  expect(gemDisplayName(summoner, 'zh-CN')).toBe('召唤伙伴');
  expect(gemDisplayName(summoner, 'zh-TW')).toBe('召喚夥伴');
  expect(gemDisplayName({ ...summoner, name: 'Spectre: {0}' }, 'en-US')).toBe('Spectre');
  expect(gemDisplayName({ ...summoner, name: 'Command: Charge' }, 'en-US')).toBe('Command: Charge');
});

test('synthetic Thorns and Explode groups get interface labels, only for those ids', () => {
  expect(skillDisplayName('ThornsPlayer', 'en-US', catalog)).toBe('Thorns');
  expect(skillDisplayName('ThornsPlayer', 'zh-CN', catalog)).toBe('荆棘');
  expect(skillDisplayName('ThornsPlayer', 'zh-TW', catalog)).toBe('荊棘');
  expect(skillDisplayName('EnemyExplode', 'en-US', catalog)).toBe('Explode');
  expect(skillDisplayName('EnemyExplode', 'zh-CN', catalog)).toBe('爆炸');
  expect(skillDisplayName('EnemyExplode', 'zh-TW', catalog)).toBe('爆炸');
  expect(skillDisplayName('OtherEnemyThing', 'zh-CN', catalog)).toBe('Other Enemy Thing（未翻译）');
  const gem = { ...wind, skill_id: 'ThornsPlayer', name: 'Real Thorns', name_zh_cn: '真荆棘' };
  expect(skillDisplayName('ThornsPlayer', 'zh-CN', new Map([[gem.skill_id, gem]]))).toBe('真荆棘');
});


test('gem tiers have one space and missing translations retain their original name with a marker', () => {
  expect(gemDisplayName({ ...wind, name: 'Rapid Bloom II', name_zh_cn: '迅猛绽放II' }, 'zh-CN')).toBe('迅猛绽放 II');
  expect(gemDisplayName({ ...wind, name: 'Rapid Bloom   II' }, 'en-US')).toBe('Rapid Bloom II');
  expect(gemDisplayName({ ...wind, name: 'New Gem', name_zh_cn: 'New Gem' }, 'zh-CN')).toBe('New Gem（未翻译）');
  expect(gemDisplayName({ ...wind, name: 'New Gem', name_zh_tw: 'New Gem' }, 'zh-TW')).toBe('New Gem（尚未翻譯）');
});

test('form labels use known forms and shared gem names, preserving unknown labels', () => {
  expect(skillFormName('Projectile', 'form', 'zh-CN', catalog)).toBe('投射物');
  expect(skillFormName('Explosion', 'form', 'zh-TW', catalog)).toBe('爆炸');
  expect(skillFormName('Wind Dancer', wind.skill_id, 'zh-CN', catalog)).toBe('风舞者');
  expect(skillFormName('Unknown Form', 'unknown', 'zh-CN', catalog)).toBe('Unknown Form（未翻译）');
});

test('untranslated form labels keep their distinct form instead of being renamed to their parent gem', () => {
  expect(skillFormName('Untranslated Form', wind.skill_id, 'zh-CN', catalog)).toBe('Untranslated Form（未翻译）');
  expect(skillDisplayName('ImportedUnknownId', 'zh-CN', catalog, 'Original gem name')).toBe('Original gem name（未翻译）');
});
