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
  'tables/BaseItemTypes.json',
  'tables/ActiveSkills.json',
  'tables/Characters.json',
  'tables/Ascendancy.json',
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

// --- 词条行模板对：forms[{en,zh}] → [{src, en}]，src 去重（首个胜出），按 src 排序 ---
const statLines = read('lookup/stat_lines.json');
const seen = new Map();
let dupes = 0;
for (const block of statLines) {
  for (const form of block.forms) {
    if (!form.zh || !form.en) continue;
    const src = form.zh.trim();
    const en = form.en.trim();
    if (seen.has(src)) {
      if (seen.get(src) !== en) dupes += 1;
      continue;
    }
    seen.set(src, en);
  }
}
const pairs = [...seen.entries()]
  .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
  .map(([src, en]) => ({ src, en }));

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
write('_meta.json', {
  source: 'https://github.com/addohm/poe2-en-cn-dict',
  source_generated_at: upstreamMeta.generatedAt ?? null,
  regen_command: 'node pipeline/gen-zh-cn.mjs',
  counts: {
    base_items: Object.keys(baseItems).length,
    skills: Object.keys(skills).length,
    stat_lines: pairs.length,
    ambiguous_src_dropped: dupes,
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
  `zh-CN 生成完成：base_items ${Object.keys(baseItems).length} / skills ${Object.keys(skills).length} / stat_lines ${pairs.length}（同文异译丢弃 ${dupes}）→ ${outDir}`,
);
