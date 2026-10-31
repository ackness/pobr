import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { expect, it } from 'vitest';
import generated from './supportDescriptions.zh-CN.json';
import { supportDescription } from './supportDescriptions';
import type { TradeCatalog } from './tradeOptimizer';

const version = readFileSync(new URL('../../../data/CURRENT', import.meta.url), 'utf8').trim();
const catalogBytes = readFileSync(new URL(`../../../data/${version}/overlay/trade_catalog.json`, import.meta.url));
const catalog = JSON.parse(catalogBytes.toString()) as TradeCatalog;
const gems = new Map(catalog.gems!.map(gem => [gem.skill_id, gem]));

it('keeps generation tied to the active catalog and its existing pinned dictionary', () => {
  const meta = JSON.parse(readFileSync(new URL(`../../../data/${version}/i18n/zh-CN/_meta.json`, import.meta.url), 'utf8'));
  expect(generated._meta.data_version).toBe(version);
  expect(generated._meta.source_commit).toBe(meta.source_commit);
  expect(generated._meta.catalog_sha256).toBe(createHash('sha256').update(catalogBytes).digest('hex'));
  expect(Object.keys(generated.entries).length).toBeGreaterThan(500);
  expect([...Object.keys(generated.entries), ...generated.untranslated].sort()).toEqual(
    catalog.gems!.filter(gem => gem.is_support).map(gem => gem.skill_id).sort());
});

it.each([
  ['SupportRakiatasFlowPlayer', '元素抗性视为反转'],
  ['SupportGarukhansResolvePlayer', '分叉暴击率'],
  ['SupportElementalArmamentPlayerTwo', '更高元素伤害'],
  ['SupportElementalFocusPlayer', '无法造成元素异常状态'],
  ['SupportInevitableCriticalsPlayer', '持续累积暴击几率'],
])('localizes the screenshot support %s without changing the catalog', (id, text) => {
  const gem = Object.freeze({ ...gems.get(id)! });
  const before = JSON.stringify(gem);
  expect(supportDescription(gem, 'zh-CN')).toMatchObject({ source: 'dictionary', lang: 'zh-CN', text: expect.stringContaining(text) });
  expect(supportDescription(gem, 'zh-TW')).toMatchObject({ source: 'dictionary', lang: 'zh-TW', text: expect.stringContaining('輔助') });
  expect(supportDescription(gem, 'en-US')).toEqual({ source: 'english', lang: 'en', text: gem.description });
  expect(JSON.stringify(gem)).toBe(before);
});

it('covers every generated support and its tier copies with complete prose, retaining fallback English', () => {
  for (const [id, entry] of Object.entries(generated.entries)) {
    const gem = gems.get(id)!;
    expect(supportDescription(gem, 'zh-CN').text).toBe(entry.zh_cn);
    expect(entry.zh_cn).toMatch(/\p{Script=Han}/u);
    expect(entry.zh_cn).not.toMatch(/[\[\]]/);
    expect(supportDescription(gem, 'en-US').text).toBe(gem.description);
  }
  for (const id of generated.untranslated) {
    const gem = gems.get(id)!;
    expect(supportDescription(gem, 'zh-CN').source).toBe(gem.description?.trim() ? 'english' : 'unavailable');
  }
});

it('falls back for missing IDs, changed source prose and missing descriptions in both Chinese locales', () => {
  const gem = gems.get('SupportRakiatasFlowPlayer')!;
  for (const lang of ['zh-CN', 'zh-TW'] as const) {
    for (const input of [{ ...gem, skill_id: 'future-support' }, { ...gem, description: gem.description + ' Changed.' }]) {
      expect(supportDescription(input, lang)).toEqual({ text: input.description, lang: 'en', source: 'english' });
    }
    expect(supportDescription({ skill_id: gem.skill_id }, lang)).toEqual({ text: '', lang, source: 'unavailable' });
  }
});
