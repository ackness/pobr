#!/usr/bin/env node
// 从固定提交的国服 WeGame ⇄ 国际服词典生成简中数据：
//   data/<CURRENT>/i18n/zh-CN/base_items.json   基底/宝石名边车（id → 简中名，与 zh-TW 同构）
//   data/<CURRENT>/i18n/zh-CN/skills.json       主动技能名边车（id → 简中名）
//   data/<CURRENT>/i18n/zh-CN/stat_lines.json   词条行模板对 [{src, en}]（中文词条输入翻译用）
//   data/<CURRENT>/i18n/zh-CN/_meta.json        来源与统计
// 并把 manifest.json 的 languages 追加 zh-CN。
//
// Usage: node pipeline/gen-zh-cn.mjs [--version <version>] [--dict <directory> | --ref <sha> | --refresh]
//        [--translations-dir <repository> | --translations-ref <sha>]
// Both upstreams are pinned independently; --refresh resolves each HEAD once.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { DICTIONARY_REPOSITORY, dictionarySource } from './dictionary-source.mjs';
import { TRANSLATION_REPOSITORY, translationSource, mergeTranslations } from './translation-source.mjs';
const FILES = [
  'lookup/stat_lines.json',
  'lookup/en_to_cn.json',
  'tables/BaseItemTypes.json',
  'tables/ActiveSkills.json',
  'tables/Characters.json',
  'tables/Ascendancy.json',
  'tables/Words.json',
  'tables/PassiveSkills.json',
  'tables/Mods.json',
  'meta.json',
];

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.join(scriptDir, '..');
if (process.env.POBR_CANDIDATE !== '1') {
  const result = spawnSync('python3', [path.join(scriptDir, 'data_snapshot.py'), 'dictionary', '--', ...process.argv.slice(2)],
    { cwd: repoRoot, stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
const options = {};
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (arg === '--refresh') options.refresh = true;
  else if (['--dict', '--ref', '--version', '--translations-dir', '--translations-ref'].includes(arg) && process.argv[i + 1]) options[arg.slice(2)] = process.argv[++i];
  else throw new Error(`Unknown or incomplete option: ${arg}`);
}
if ([options.dict, options.ref, options.refresh].filter(Boolean).length > 1) {
  throw new Error('Choose one of --dict, --ref or --refresh.');
}
if ([options['translations-dir'], options['translations-ref'], options.refresh].filter(Boolean).length > 1) {
  throw new Error('Choose one of --translations-dir, --translations-ref or --refresh.');
}
const version = options.version ?? fs.readFileSync(path.join(repoRoot, 'data/CURRENT'), 'utf8').split('\n')[0].trim();
if (!/^[0-9]+(?:\.[0-9]+)+$/.test(version)) throw new Error('Invalid data version.');
const dataRoot = process.env.POBR_DATA_ROOT ?? path.join(repoRoot, 'data');
const outDir = path.join(dataRoot, version, 'i18n', 'zh-CN');
const metaPath = path.join(outDir, '_meta.json');
const previous = fs.existsSync(metaPath) ? JSON.parse(fs.readFileSync(metaPath, 'utf8')) : {};
let dictDir = options.dict;
let sourceCommit = null;
if (!dictDir) {
  const snapshot = await dictionarySource({ cacheRoot: path.join(scriptDir, '.cache', 'zh-cn-dict'),
    files: FILES, ref: options.ref ?? previous.source_commit, refresh: options.refresh });
  dictDir = snapshot.directory;
  sourceCommit = snapshot.ref;
}
const translationPin = JSON.parse(fs.readFileSync(path.join(scriptDir, 'translation-source.json'), 'utf8'));
if (translationPin.repository !== TRANSLATION_REPOSITORY || !/^[a-f0-9]{40}$/.test(translationPin.commit)) {
  throw new Error('Invalid independent translation source pin.');
}
const independent = await translationSource({
  cacheRoot: path.join(scriptDir, '.cache', 'independent-translations'),
  directory: options['translations-dir'],
  ref: options['translations-dir'] ? undefined : options['translations-ref'] ?? previous.translation_source?.commit ?? translationPin.commit,
  refresh: Boolean(options.refresh),
});

const read = (rel) => JSON.parse(fs.readFileSync(path.join(dictDir, rel), 'utf8'));

// --- 名词边车：表条目 → { id: 简中名 }（与 zh-TW 边车同构，键排序保证 diff 友好）---
function nameSidecar(table, column) {
  const out = {};
  for (const entry of table.entries) {
    const cell = entry.columns[column];
    const zh = cell?.[0]?.zh;
    if (zh) out[entry.id] = zh;
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const baseItems = nameSidecar(read('tables/BaseItemTypes.json'), 'Name');
const skills = nameSidecar(read('tables/ActiveSkills.json'), 'DisplayedName');

// --- 名词直译表（GGG Words 表：唯一物品名等专有名词，英文名 → 简中名；
// 合入 ActiveSkills 显示名，供 `Grants Skill: …` 行的技能名捕获二次翻译）---
const words = {
  ...enToZh(read('tables/ActiveSkills.json'), 'DisplayedName'),
  ...enToZh(read('tables/Words.json'), 'Text2'),
};

// --- 天赋节点名（PassiveSkills 表 Name 列，英文名 → 简中名；树 tooltip 用）---
const passiveNames = enToZh(read('tables/PassiveSkills.json'), 'Name');

// --- 词缀名（Mods 表 Name 列，英文名 → 简中名；魔法物品名「后缀之+前缀的+基底」
// 组合翻译用，中文缀名自带「…的/…之」格式）---
const affixNames = enToZh(read('tables/Mods.json'), 'Name');

// --- RARE 随机名组成词：通用词典的单词条目（首字母大写单词 → 短中文名词）。
// RARE 名 = 前缀词 + 后缀词，国服按中文连写组合（Storm Bite → 风暴慧齿）；
// Words 表只收录了前缀词，后缀词从通用词典补齐。---
const rareWords = {};
for (const [en, zh] of Object.entries(read('lookup/en_to_cn.json'))) {
  if (
    /^[A-Z][a-zA-Z']*$/.test(en) &&
    typeof zh === 'string' &&
    zh.length <= 8 &&
    /^[一-鿿]+$/.test(zh)
  ) {
    rareWords[en] = zh;
  }
}
// Reviewed rare-name fallbacks are maintained in the independent translation repository.
const wordPatches = independent.bundle.words.map(({ english, chinese }) => {
  rareWords[english] ??= chinese;
  return { english, proposed: chinese, retained: rareWords[english] };
});
const rareWordsSorted = Object.fromEntries(
  Object.entries(rareWords).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
);


// --- 词条行模板对：forms[{en,zh}] → [{src, en}]，按 (src, en) 对去重后排序。
// 同一中文对应的全部英文变体（单复数/大小写措辞）都保留——en→zh 显示方向按
// 英文字面量匹配，丢变体=丢翻译；zh→en 方向同 src 多候选按序首中胜出，行为不变。
const statLines = read('lookup/stat_lines.json');
const seen = new Set();
const pairs = [];
for (const block of statLines) {
  for (const form of block.forms) {
    if (!form.zh || !form.en) continue;
    const src = form.zh.trim();
    const en = form.en.trim();
    const key = `${src}\x00${en}`;
    if (seen.has(key)) continue;
    seen.add(key);
    pairs.push({ src, en });
  }
}
const imported = mergeTranslations(pairs, independent.bundle);

// --- 职业/升华名：Characters 按英文名、Ascendancy 按英文名（前端 UI 用英文
// canonical 名索引；泰坦等 23 个升华 + 全部可选职业）---
function enToZh(table, column) {
  const out = {};
  for (const entry of table.entries) {
    const cell = entry.columns[column]?.[0];
    if (cell?.en && cell?.zh) out[cell.en] = cell.zh;
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const classNames = {
  classes: enToZh(read('tables/Characters.json'), 'Name'),
  ascendancies: enToZh(read('tables/Ascendancy.json'), 'Name'),
};

const upstreamMeta = read('meta.json');

fs.mkdirSync(outDir, { recursive: true });
const write = (name, value) =>
  fs.writeFileSync(path.join(outDir, name), JSON.stringify(value, null, 2) + '\n');
write('base_items.json', baseItems);
write('skills.json', skills);
write('stat_lines.json', imported.pairs);
write('classes.json', classNames);
write('words.json', words);
write('passive_names.json', passiveNames);
write('mods.json', affixNames);
write('rare_words.json', rareWordsSorted);
write('_meta.json', {
  source: `https://github.com/${DICTIONARY_REPOSITORY}`,
  source_commit: sourceCommit,
  source_generated_at: upstreamMeta.generatedAt ?? null,
  translation_source: independent.receipt,
  regen_command: `node pipeline/gen-zh-cn.mjs --version ${version} ${sourceCommit ? `--ref ${sourceCommit}` : '--dict <dictionary-dir>'} ${independent.receipt.commit ? `--translations-ref ${independent.receipt.commit}` : '--translations-dir <repository-dir>'}`,
  counts: {
    base_items: Object.keys(baseItems).length,
    skills: Object.keys(skills).length,
    stat_lines: imported.pairs.length,
    words: Object.keys(words).length,
    passive_names: Object.keys(passiveNames).length,
    mods: Object.keys(affixNames).length,
    rare_words: Object.keys(rareWordsSorted).length,
    curated_templates: imported.report.curated_templates,
    added_trade_templates: imported.report.added_trade,
  },
});
const auditDirectory = path.join(dataRoot, version, 'generated');
fs.mkdirSync(auditDirectory, { recursive: true });
fs.writeFileSync(path.join(auditDirectory, 'translation-audit.json'), JSON.stringify({
  schema_version: 1, source: independent.receipt, ...imported.report, word_patches: wordPatches,
}, null, 2) + '\n');

// manifest languages 追加 zh-CN。
const manifestPath = path.join(dataRoot, version, 'manifest.json');
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
if (!manifest.languages.includes('zh-CN')) {
  manifest.languages.push('zh-CN');
  manifest.languages.sort();
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
}

console.log(
  `zh-CN 生成完成：base_items ${Object.keys(baseItems).length} / skills ${Object.keys(skills).length} / stat_lines ${imported.pairs.length}（独立词表维护 ${imported.report.curated_templates}，新增交易模板 ${imported.report.added_trade}）→ ${outDir}`,
);
