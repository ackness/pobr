import { expect, test } from 'vitest';
import type { GemCatalogEntry } from '../../api/types';
import { gemDisplayName, rankGemResults } from './GemPicker';

// Production catalog shape: skill_id is a granted effect ID, not a gem base ID.
const spark: GemCatalogEntry = {
  skill_id: 'SparkPlayer', name: 'Spark', name_zh_cn: '电球', name_zh_tw: '電球',
  colour: 'int', is_support: false, is_lineage: false, is_unique_only: false,
  active_effect_ids: ['SparkPlayer'], additional_skill_ids: ['SparkColdInfusedPlayer'],
  effect_forms: [{ skill_id: 'SparkPlayer', stat_sets: [
    { id: 'SparkPlayer', index: 1, label: 'Base', stage: null },
    { id: 'SparkColdInfusedPlayer', index: 2, label: 'Cold-Infused', stage: null },
  ] }],
  tags: ['Damage', 'Duration', 'Invokable', 'Lightning', 'Mineable', 'Multicastable', 'Projectile',
    'ProjectilesFromUser', 'Spell', 'Totemable', 'Trappable', 'Triggerable', 'Unleashable', 'UsableWhileMoving'],
};
const earthboundSpark: GemCatalogEntry = {
  skill_id: 'UniqueEarthboundTriggeredSparkPlayer', name: 'Spark', name_zh_cn: '电球', name_zh_tw: '電球',
  colour: 'int', is_support: false, is_lineage: false, is_unique_only: true,
  active_effect_ids: ['UniqueEarthboundTriggeredSparkPlayer'],
  effect_forms: [{ skill_id: 'UniqueEarthboundTriggeredSparkPlayer', stat_sets: [] }],
  tags: [...spark.tags, 'Cold', 'InbuiltTrigger', 'Triggered'].sort(),
};

for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
  test(`only socketable Spark appears in picker options in ${lang}, retaining imported skill metadata`, () => {
    const entries = [earthboundSpark, spark];
    const snapshot = structuredClone(entries);
    const name = gemDisplayName(spark, lang);
    for (const query of ['', 'Spark', name]) {
      expect(rankGemResults(entries, query, 'all', lang)).toEqual([spark]);
    }
    expect(rankGemResults(entries, name, 'int', lang).map(entry => gemDisplayName(entry, lang))).toEqual([name]);
    expect(rankGemResults(entries, name, 'str', lang)).toEqual([]);
    expect(entries).toEqual(snapshot);
    expect(entries[0]).toBe(earthboundSpark);
    expect(gemDisplayName(earthboundSpark, lang)).toBe(name);
  });
}

test('trigger tags alone do not exclude a socketable gem', () => {
  const trigger = { ...spark, skill_id: 'CastOnShockPlayer', name: 'Cast on Shock', tags: ['Meta', 'Trigger', 'Triggered'] };
  expect(rankGemResults([trigger], '', 'all', 'en-US')).toEqual([trigger]);
});

const gem = (id: string, cn: string, en = id, tags: string[] = []): GemCatalogEntry => ({
  skill_id: id, name: en, name_zh_cn: cn, name_zh_tw: cn, tags, colour: 'int',
  active_effect_ids: [id], is_support: false, is_lineage: false, is_unique_only: false,
});

test('Chinese name prefixes and substrings outrank English names and tags, with duplicates removed', () => {
  const tag = gem('aaa', '灼烧', 'A tag match', ['Fire']);
  const english = gem('bbb', '其他', '火 English match');
  const substring = gem('ccc', '烈火', 'Z substring');
  const prefix = gem('ddd', '火球', 'Z prefix');
  const entries = [tag, english, substring, prefix, prefix, gem('none', '冰球')];
  expect(rankGemResults(entries, '火', 'all', 'zh-CN').map(gem => gem.skill_id)).toEqual(['ddd', 'ccc', 'bbb', 'aaa']);
  expect(rankGemResults(entries, '火', 'dex', 'zh-CN')).toEqual([]);
  expect(entries).toHaveLength(6);
});

test('empty searches keep the full distinct result count, including more than 200 gems', () => {
  const entries = Array.from({ length: 205 }, (_, index) => gem(`gem-${index}`, `宝石 ${index}`));
  expect(rankGemResults([...entries, entries[0]], '', 'all', 'zh-CN')).toHaveLength(205);
});

test('tier searches match names both with and without the displayed Roman numeral spacing', () => {
  const entries = [gem('bloom', '迅猛绽放II', 'Rapid Bloom II')];
  for (const query of ['绽放II', '绽放 II', 'BloomII', 'Bloom II']) {
    expect(rankGemResults(entries, query, 'all', 'zh-CN')).toHaveLength(1);
  }
});

for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
  test(`both distinct Chinese names remain searchable with name matches before English and tags in ${lang}`, () => {
    const fireball = { ...gem('fireball', '火球', 'Fireball'), name_zh_tw: '熾焰球' };
    for (const query of ['火球', '熾焰球']) {
      const substring = { ...gem('substring', `大${query}`, 'Other'), name_zh_tw: `大${query}` };
      const english = gem('english', '其他', `${query} English`);
      const results = rankGemResults([english, substring, fireball], query, 'all', lang);
      expect(results.map(entry => entry.skill_id)).toEqual(['fireball', 'substring', 'english']);
    }
    expect(rankGemResults([fireball], '焰球', 'all', lang)).toEqual([fireball]);
  });
}

for (const [lang, first] of [['en-US', 'fire'], ['zh-CN', 'ice'], ['zh-TW', 'fire']] as const) {
  test(`an empty search starts with the first localized name in ${lang}`, () => {
    const entries = [gem('fire', '火球', 'Fireball'), gem('ice', '冰霜新星', 'Ice Nova'),
      { ...gem('lightning', '电球', 'Spark'), name_zh_tw: '電球' }];
    expect(rankGemResults(entries, '', 'all', lang)[0].skill_id).toBe(first);
  });
}
