#!/usr/bin/env node
// 从 addohm/poe2-en-cn-dict（国服 WeGame ⇄ 国际服词典）生成简中数据（TODO Phase 7）：
//   data/<CURRENT>/i18n/zh-CN/base_items.json   基底/宝石名边车（id → 简中名，与 zh-TW 同构）
//   data/<CURRENT>/i18n/zh-CN/skills.json       主动技能名边车（id → 简中名）
//   data/<CURRENT>/i18n/zh-CN/stat_lines.json   词条行模板对 [{src, en}]（中文词条输入翻译用）
//   data/<CURRENT>/i18n/zh-CN/_meta.json        来源与统计
// 并把 manifest.json 的 languages 追加 zh-CN。
//
// 用法：node gen-zh-cn.mjs [--dict <本地词典目录>]
//   缺省从 GitHub raw 下载三个文件到 .cache/zh-cn-dict/（词典成品直接提交在上游仓库，
//   无需安装国服客户端）。注意：词典按上游当前补丁生成，与本仓库数据版本可能存在
//   小版本偏差（与 vendor overlay 同性质，_meta 记录来源 commit 日期供追溯）。

import fs from 'node:fs';
import path from 'node:path';

const RAW = 'https://raw.githubusercontent.com/addohm/poe2-en-cn-dict/master/dictionary';
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

const repoRoot = path.join(import.meta.dirname, '..');
const version = fs.readFileSync(path.join(repoRoot, 'data/CURRENT'), 'utf8').split('\n')[0].trim();
const outDir = path.join(repoRoot, 'data', version, 'i18n', 'zh-CN');

const dictArg = process.argv.indexOf('--dict');
let dictDir = dictArg > -1 ? process.argv[dictArg + 1] : null;

if (!dictDir) {
  dictDir = path.join(import.meta.dirname, '.cache', 'zh-cn-dict');
  for (const rel of FILES) {
    const dest = path.join(dictDir, rel);
    if (fs.existsSync(dest)) continue;
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    const url = `${RAW}/${rel}`;
    console.log(`下载 ${url}`);
    const res = await fetch(url);
    if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
    fs.writeFileSync(dest, Buffer.from(await res.arrayBuffer()));
  }
}

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
// 通用词典漏收的组成词补丁（措辞取自官方文本同词根，如「消融漩涡」）。
rareWords.Maelstrom ??= '漩涡';
const rareWordsSorted = Object.fromEntries(
  Object.entries(rareWords).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
);

// --- 本地补丁模板对（上游词典缺失；措辞参考国服客户端同族词条 / poe2db）---
// 表双向使用：src=zh 供输入翻译（zh→en），运行时换向后 en 侧供显示翻译（en→zh）。
const EXTRA_PAIRS = [
  // 树专属文本（非 stat description，上游词典不收录）。
  { src: '获得 {0} 个天赋技能点', en: 'Grants {0} Passive Skill Point' },
  { src: '获得 {0} 个天赋技能点', en: 'Grants {0} Passive Skill Points' },
  {
    src: '100 个天赋技能点转变为武器组技能点',
    en: '100 Passive Skill Points become Weapon Set Skill Points',
  },
  // Time-Lost 珠宝单行变体（上游只有带 containing 从句的多行版）。
  {
    src: '珠宝插槽天赋效果提高 {0}%',
    en: '{0}% increased Effect of Jewel Socket Passive Skills',
  },
  // 符文纽带行前缀（内层词条经字符串占位符捕获二次翻译）。
  { src: '纽带：{0}', en: 'Bonded: {0}' },
  // 物品附赠技能行（措辞对齐 poe2db「获得技能」；技能名经名词表二次翻译）。
  { src: '获得技能：{0} 级 {1}', en: 'Grants Skill: Level {0} {1}' },
  { src: '获得技能：{0}', en: 'Grants Skill: {0}' },
  // 范围珠宝行：grant 后接任意词条（上游只有具体词条版），嵌套词条二次翻译。
  {
    src: '范围内的核心天赋还会获得：{0}',
    en: 'Notable Passive Skills in Radius also grant {0}',
  },
  {
    src: '范围内的小型天赋还会获得：{0}',
    en: 'Small Passive Skills in Radius also grant {0}',
  },
  // 范围珠宝 keystone 行（上游为多行合并模板，物品文本按行拆开送翻）。
  { src: '{0}范围内的天赋可以配置', en: 'Passives in Radius of {0} can be Allocated' },
  { src: '而无需连结至你的天赋树', en: 'without being connected to your tree' },
];

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
for (const pair of EXTRA_PAIRS) {
  const key = `${pair.src}\x00${pair.en}`;
  if (!seen.has(key)) {
    seen.add(key);
    pairs.push(pair);
  }
}
pairs.sort((a, b) => (a.src < b.src ? -1 : a.src > b.src ? 1 : a.en < b.en ? -1 : 1));

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
write('stat_lines.json', pairs);
write('classes.json', classNames);
write('words.json', words);
write('passive_names.json', passiveNames);
write('mods.json', affixNames);
write('rare_words.json', rareWordsSorted);
write('_meta.json', {
  source: 'https://github.com/addohm/poe2-en-cn-dict',
  source_generated_at: upstreamMeta.generatedAt ?? null,
  regen_command: 'node pipeline/gen-zh-cn.mjs',
  counts: {
    base_items: Object.keys(baseItems).length,
    skills: Object.keys(skills).length,
    stat_lines: pairs.length,
    words: Object.keys(words).length,
    passive_names: Object.keys(passiveNames).length,
    mods: Object.keys(affixNames).length,
    rare_words: Object.keys(rareWordsSorted).length,
    extra_pairs: EXTRA_PAIRS.length,
  },
});

// manifest languages 追加 zh-CN。
const manifestPath = path.join(repoRoot, 'data', version, 'manifest.json');
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
if (!manifest.languages.includes('zh-CN')) {
  manifest.languages.push('zh-CN');
  manifest.languages.sort();
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
}

console.log(
  `zh-CN 生成完成：base_items ${Object.keys(baseItems).length} / skills ${Object.keys(skills).length} / stat_lines ${pairs.length}（含补丁 ${EXTRA_PAIRS.length}）→ ${outDir}`,
);
