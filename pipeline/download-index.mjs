#!/usr/bin/env node
// 弹性分块下载 GGG bundle 索引 `_.index.bin` 到 pathofexile-dat 期望的缓存路径。
//
// 背景：pathofexile-dat 取索引用单条 fetch + 无断点续传；GGG 的索引达 ~113MB，
// 在不稳定网络下单流极易中途断开（UND_ERR_SOCKET），一断即全失败。本脚本用
// HTTP Range 分块 + 每块重试下载，写入 `.cache/<patch>/_.index.bin`。由于
// pathofexile-dat 在 `.cache/<patch>/` 已存在时不会重下索引（见其 CdnBundleLoader），
// 预热缓存后再跑 `npx pathofexile-dat` 即可跳过索引下载、只取体积小的表 bundle。
//
// 用法：在本目录（含 config.json）运行 `node download-index.mjs`，随后运行 `npx pathofexile-dat`。

import fs from 'node:fs';
import path from 'node:path';

const CHUNK = 4 * 1024 * 1024; // 4MB
const MAX_RETRY = 6;

const config = JSON.parse(fs.readFileSync(path.join(process.cwd(), 'config.json'), 'utf-8'));
const patch = config.patch;
if (!patch) {
  console.error('config.json 缺少 "patch" 版本号；本地 Steam 模式无需预热索引。');
  process.exit(1);
}
// 版本号以 "4." 开头走 PoE2 CDN，否则 PoE1，与 pathofexile-dat 的判定一致。
const cdn = patch.startsWith('4.') ? 'https://patch-poe2.poecdn.com' : 'https://patch.poecdn.com';
const url = `${cdn}/${patch}/Bundles2/_.index.bin`;
const outDir = path.join(process.cwd(), '.cache', patch);
const out = path.join(outDir, '_.index.bin');

fs.mkdirSync(outDir, { recursive: true });

const head = await fetch(url, { headers: { Range: 'bytes=0-0' } });
// 已完整缓存则直接跳过（让一键链路可重复运行而不重下 ~113MB）。
{
  const expected = Number((head.headers.get('content-range') || '').split('/')[1]);
  if (expected && fs.existsSync(out) && fs.statSync(out).size === expected) {
    console.log(`索引已缓存且完整（${(expected / 1e6).toFixed(1)} MB），跳过下载。`);
    process.exit(0);
  }
}
const total = Number((head.headers.get('content-range') || '').split('/')[1]);
if (!total) {
  console.error(`无法取得索引大小（status ${head.status}）。检查 patch 版本是否正确：${patch}`);
  process.exit(1);
}
console.log(`索引 ${url}\n总大小 ${(total / 1e6).toFixed(1)} MB，按 ${CHUNK / 1e6}MB 分块下载…`);

const fd = fs.openSync(out, 'w');
fs.ftruncateSync(fd, total);
let done = 0;
for (let start = 0; start < total; start += CHUNK) {
  const end = Math.min(start + CHUNK - 1, total - 1);
  let ok = false;
  for (let attempt = 0; attempt < MAX_RETRY && !ok; attempt++) {
    try {
      const r = await fetch(url, { headers: { Range: `bytes=${start}-${end}` } });
      if (!(r.status === 206 || r.status === 200)) throw new Error(`status ${r.status}`);
      const buf = Buffer.from(await r.arrayBuffer());
      fs.writeSync(fd, buf, 0, buf.length, start);
      done += buf.length;
      ok = true;
    } catch (e) {
      if (attempt === MAX_RETRY - 1) {
        console.error(`块 @${start} 重试耗尽：${e.message}`);
        fs.closeSync(fd);
        process.exit(1);
      }
      await new Promise((s) => setTimeout(s, 500 * (attempt + 1)));
    }
  }
  if ((start / CHUNK) % 5 === 0) {
    console.log(`  ${(done / 1e6).toFixed(0)}/${(total / 1e6).toFixed(0)} MB`);
  }
}
fs.closeSync(fd);
const size = fs.statSync(out).size;
if (size !== total) {
  console.error(`大小不符：${size} != ${total}`);
  process.exit(1);
}
console.log(`完成：${out}（${(size / 1e6).toFixed(1)} MB）。现在可运行 \`npx pathofexile-dat\`。`);
