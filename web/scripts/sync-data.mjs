// 把仓库 data/<version>/ 的 JSON 同步到 web/public/data/<version>/，并生成
// manifest.json（文件清单——wasm 数据注入需要完整相对路径列表）。
// 用法：npm run sync-data（可用 POBR_DATA_VERSION 覆盖版本）。
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const webRoot = fileURLToPath(new URL('..', import.meta.url));
const repoRoot = join(webRoot, '..');
const dataRoot = join(repoRoot, 'data');

const version =
  process.env.POBR_DATA_VERSION ??
  readFileSync(join(dataRoot, 'CURRENT'), 'utf8').split('\n')[0].trim();

const src = join(dataRoot, version);
const destRoot = join(webRoot, 'public', 'data');
const dest = join(destRoot, version);

function walkJson(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      out.push(...walkJson(path));
    } else if (entry.endsWith('.json')) {
      out.push(path);
    }
  }
  return out;
}

rmSync(destRoot, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });

const files = [];
for (const path of walkJson(src)) {
  const rel = relative(src, path).split('\\').join('/');
  cpSync(path, join(dest, rel));
  files.push(rel);
}
files.sort();

writeFileSync(join(destRoot, 'manifest.json'), JSON.stringify({ version, files }, null, 2));
console.log(`synced ${files.length} files from data/${version} -> web/public/data/${version}`);
