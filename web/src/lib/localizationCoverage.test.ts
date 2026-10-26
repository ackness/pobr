import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { describe, expect, test } from 'vitest';
import type { ConfigCatalogFile, ConfigOption, GemCatalogEntry } from '../api/types';
import gemFixture from '../fixtures/gem_catalog.json';
import calculateFixture from '../fixtures/calculate.json';
import declaredInputs from '../fixtures/detail_inputs.json';
import { DETAIL_LABELS } from './detailLabels';
import { STAT_SECTIONS } from './statDisplay';
import { statNameLabel, originKindLabel, modTypeLabel, originLabel, bootMessageLabel, t, type Lang, type UiKey } from './i18n';
import { configLabel, configListOptionLabel, configTooltip, stripConfigMarkup } from './configLabels';
import { chineseScript } from './chinese';
import { gemDisplayName } from './skillNames';
import helpTranslations from './configHelp.zh-CN.json';
import gameTerms from './configGameTerms.json';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const read = (path: string): string => readFileSync(join(root, path), 'utf8');
const version = read('data/CURRENT').trim();
const catalog = (JSON.parse(read(`data/${version}/overlay/config_options.json`)) as ConfigCatalogFile).options;
const shared = readdirSync(join(root, 'data/overlay-common'))
  .filter(file => file.endsWith('_config_options.json'))
  .flatMap(file => (JSON.parse(read(`data/overlay-common/${file}`)) as ConfigCatalogFile).options);
const options: ConfigOption[] = [...catalog, ...shared];
const chinese: Lang[] = ['zh-CN', 'zh-TW'];

function constantStrings(source: string, constant: string): string[] {
  const body = source.match(new RegExp(`pub const ${constant}:[^=]*=\\s*&\\[([\\s\\S]*?)\\];`))?.[1];
  expect(body, constant).toBeDefined();
  return [...body!.matchAll(/"([^"]+)"/g)].map(match => match[1]);
}

// Inspect declarations without building Cargo or relying only on the subset
// present in a frozen build. Dynamic utility slots and shared STATS are included.
function displayIds(): Set<string> {
  const source = read('crates/pobr-core/src/display_catalog.rs');
  const ids = new Set([...source.matchAll(/computed\(\s*"([^"]+)"/g)].map(match => match[1]));
  expect(ids.size).toBeGreaterThan(500);
  for (const match of source.matchAll(/for id in crate::calc::(\w+)::STATS/g)) {
    for (const id of constantStrings(read(`crates/pobr-core/src/calc/${match[1]}.rs`), 'STATS')) ids.add(id);
  }
  const utility = read('crates/pobr-core/src/calc/utility.rs');
  const global = utility.match(/pub const GLOBAL_STATS:[^=]*=\s*&\[([\s\S]*?)\];/)![1];
  for (const match of global.matchAll(/\(\s*"([^"]+)"/g)) ids.add(match[1]);
  const common = constantStrings(utility, 'ITEM_STATS');
  const charms = constantStrings(utility, 'CHARM_STATS');
  const flasks = constantStrings(utility, 'FLASK_STATS');
  for (const [kind, count, fields] of [['Flask', 2, flasks], ['Charm', 3, charms]] as const) {
    for (let slot = 1; slot <= count; slot++) {
      for (const field of [...common, ...fields]) {
        if (kind === 'Charm' && field.includes('Recovery') && !charms.includes(field)) continue;
        ids.add(`${kind}${slot}${field}`);
      }
    }
  }
  return ids;
}

describe('current data localization coverage', () => {
  test('all declared display stats, fixture stats and detail inputs have Chinese labels', () => {
    const ids = new Set([...displayIds(), ...declaredInputs, ...calculateFixture.stats.map(stat => stat.id)]);
    const missing = chinese.flatMap(lang => [...ids].filter(id => !/[\u3400-\u9fff]/u.test(statNameLabel(lang, id))).map(id => `${lang}: ${id}`));
    expect(missing).toEqual([]);
  });

  test('all current and shared configuration labels, help and list options are translated', () => {
    expect(catalog.length).toBeGreaterThan(550);
    expect(catalog.filter(option => option.tooltip).length).toBeGreaterThan(250);
    const missing: string[] = [];
    for (const lang of chinese) {
      for (const option of options) {
        if (!/[\u3400-\u9fff]/u.test(configLabel(lang, option))) missing.push(`${lang} label ${option.var}`);
        if (option.tooltip && !/[\u3400-\u9fff]/u.test(configTooltip(lang, option.var, option.tooltip) ?? '')) missing.push(`${lang} help ${option.var}`);
        for (const entry of option.list_options ?? []) {
          const localized = configListOptionLabel(lang, entry.label);
          const numeric = /^[\d.+−%-]+$/.test(entry.label);
          if (!localized || (!numeric && !/[\u3400-\u9fff]/u.test(localized))) missing.push(`${lang} option ${entry.label}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });

  test('help entries retain the exact current source as their lookup key', () => {
    const sources = new Set(options.flatMap(option => option.tooltip ? [stripConfigMarkup(option.tooltip)] : []));
    expect(Object.keys(helpTranslations).filter(source => !sources.has(source))).toEqual([]);
    expect(configTooltip('zh-CN', 'newFutureOption', 'New diagnostic text')).toBe('New diagnostic text');
    expect(configTooltip('en-US', 'enemyLevel', 'Original help')).toBe('Original help');
  });

  test('Simplified stat/UI copy does not contain accidental Traditional text', () => {
    const labels = [
      ...Object.entries(DETAIL_LABELS).map(([id, labels]) => [id, labels['zh-CN']]),
      ...STAT_SECTIONS.flatMap(section => section.rows.map(row => [row.id, row.label['zh-CN']])),
      ...options.map(option => [option.var, configLabel('zh-CN', option)]),
      ...Object.entries(helpTranslations),
    ];
    expect(labels.filter(([, value]) => chineseScript(value, 'zh-CN') !== value).map(([id]) => id)).toEqual([]);
  });

  test('Traditional generic config copy uses Traditional script and native names when available', () => {
    expect(configListOptionLabel('zh-TW', 'Max Effect')).toBe('最大效果');
    expect(configListOptionLabel('zh-TW', 'Projectile Spell')).toBe('投射物法術');
    expect(configListOptionLabel('zh-TW', '^x3F6DB3Chilling')).toBe('冰緩');
    expect(configLabel('zh-TW', { var: 'windDancerStacks', label: 'Wind Dancer stacks' })).toContain('風魔舞者');
    for (const term of gameTerms) {
      const gem = gemFixture.find(gem => gem.skill_id === term.skill_id);
      expect(gem, term.skill_id).toMatchObject({ name_zh_cn: term.cn, name_zh_tw: term.tw });
    }
    expect(configListOptionLabel('zh-TW', 'Crescendo Arrow')).toBe('強鳴箭');
    expect(configListOptionLabel('zh-CN', 'Crescendo Arrow')).toBe('渐强箭');
    const copy = options.flatMap(option => [
      configLabel('zh-TW', option),
      configTooltip('zh-TW', option.var, option.tooltip) ?? '',
      ...(option.list_options ?? []).map(entry => configListOptionLabel('zh-TW', entry.label) ?? ''),
    ]);
    // 里 is also a Traditional character in proper names. OpenCC's second
    // pass through the transliterated Tasalio name is not idempotent.
    const genericCopy = copy.map(text => text.replaceAll('塔薩里奧', 'Tasalio'));
    expect(genericCopy.filter(text => chineseScript(text, 'zh-TW') !== text)).toEqual([]);
  });

  test('every UI dictionary entry has complete locale copy and the intended script', () => {
    const source = read('web/src/lib/i18n.ts').split('const DICT = {')[1].split('} as const')[0];
    const keys = [...source.matchAll(/^  ['"]([^'"]+)['"]:\s*\{/gm)].map(match => match[1] as UiKey);
    expect(keys.length).toBeGreaterThan(700);
    const wrongScript: string[] = [];
    for (const key of keys) {
      for (const lang of ['en-US', ...chinese] as Lang[]) {
        const copy = t(lang, key);
        expect(copy.trim(), `${key}: ${lang}`).not.toBe('');
        if (chineseScript(copy, lang) !== copy) wrongScript.push(`${key}: ${lang}`);
      }
    }
    expect(wrongScript).toEqual([]);
  });

  test('normal backend loading progress is localized while raw errors stay intact', () => {
    const source = read('web/src/api/wasmBackend.ts');
    const messages = [...source.matchAll(/onProgress\?\.\('([^']+)'\)/g)].map(match => match[1]);
    expect(messages.length).toBeGreaterThanOrEqual(3);
    for (const message of ['初始化…', ...messages]) {
      expect(bootMessageLabel('en-US', message)).not.toMatch(/[\u3400-\u9fff]/u);
      expect(bootMessageLabel('zh-TW', message)).toBe(chineseScript(bootMessageLabel('zh-TW', message), 'zh-TW'));
    }
    expect(bootMessageLabel('zh-TW', '加载游戏数据 20/350…')).toBe('載入遊戲資料 20/350…');
    expect(bootMessageLabel('en-US', '加载游戏数据 20/350…')).toBe('Loading game data 20/350…');
    expect(bootMessageLabel('zh-CN', 'fetch /data/unknown: 404')).toBe('fetch /data/unknown: 404');
  });

  test('translation does not mutate catalog labels, list values, or exported stat IDs', () => {
    const before = JSON.stringify(options);
    for (const option of options) {
      configLabel('zh-TW', option);
      configTooltip('zh-CN', option.var, option.tooltip);
      option.list_options?.forEach(entry => configListOptionLabel('zh-TW', entry.label));
    }
    expect(JSON.stringify(options)).toBe(before);
    expect(statNameLabel('zh-CN', 'UnreleasedStableStatID')).toBe('UnreleasedStableStatID');
  });

  test('gem names preserve the requested locale from the real catalog', () => {
    for (const gem of gemFixture as GemCatalogEntry[]) {
      for (const lang of chinese) {
        const native = lang === 'zh-CN' ? gem.name_zh_cn : gem.name_zh_tw;
        if (native?.trim()) expect(gemDisplayName(gem, lang)).toBe(native.replace(/[:：]\s*\{0\}$/, '').trim());
      }
    }
  });

  test('all source and modifier enum variants have labels while unknown diagnostics are preserved', () => {
    const source = read('crates/pobr-data/src/source.rs').match(/pub enum SourceKind \{([\s\S]*?)\n\}/)![1];
    const kinds = [...source.matchAll(/^    (\w+),/gm)].map(match => match[1]);
    const mods = read('crates/pobr-data/src/modifier.rs').match(/pub enum ModType \{([\s\S]*?)\n\}/)![1];
    const types = [...mods.matchAll(/^    (\w+),/gm)].map(match => match[1]);
    for (const lang of chinese) {
      for (const kind of kinds) expect(originKindLabel(lang, kind), kind).toMatch(/[\u3400-\u9fff]/u);
      for (const type of types) expect(modTypeLabel(lang, type), type).toMatch(/[\u3400-\u9fff]/u);
      expect(originLabel(lang, 'item.weapon1.explicit')).toContain('主手');
      expect(originLabel(lang, 'flask.charm1')).toContain(lang === 'zh-CN' ? '护符' : '護符');
      expect(originLabel(lang, 'unknown.raw_source_ID')).toBe('unknown.raw_source_ID');
    }
    expect(modTypeLabel('zh-CN', 'INC')).toBe('提高／降低');
    expect(modTypeLabel('zh-CN', 'MORE')).toBe('总增／总降');
    expect(t('zh-TW', 'a11y.closeDialog')).toBe('關閉對話框');
    expect(t('zh-CN', 'a11y.closeDialog')).toBe('关闭对话框');
  });
});
