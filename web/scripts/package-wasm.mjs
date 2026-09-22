// Package the same WASM and data inputs used by the Web gate. No rebuild or download.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cpSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const pretty = value => JSON.stringify(value, null, 2) + '\n';
const mib = bytes => (bytes / 1024 ** 2).toFixed(3);

function safeRelative(path) {
  if (typeof path !== 'string' || !path || path.split('/').some(part => !part || part === '.' || part === '..')
      || /[\\:]/.test(path)) throw new Error(`Unsafe package path: ${path}`);
  return path;
}

function readInput(root, relative) {
  safeRelative(relative);
  let path = root;
  for (const part of relative.split('/')) {
    path = join(path, part);
    if (lstatSync(path).isSymbolicLink()) throw new Error(`Symlink in package input: ${relative}`);
  }
  return readFileSync(path);
}

export function packageWasm({ root = repoRoot, outDir = join(root, '.cache/wasm-release'), tag } = {}) {
  const pkgRoot = join(root, 'web/src/wasm/pkg');
  const pkg = json(join(pkgRoot, 'package.json'));
  const workspaceVersion = readFileSync(join(root, 'Cargo.toml'), 'utf8')
    .match(/\[workspace\.package\]\s*\nversion\s*=\s*"([^"]+)"/)?.[1];
  if (!workspaceVersion || pkg.version !== workspaceVersion) throw new Error('Stale WASM package: rebuild for the workspace version');
  if (tag && tag !== `v${pkg.version}`) throw new Error(`Tag ${tag} does not match WASM version v${pkg.version}`);
  safeRelative(pkg.version);
  const name = `pobr-wasm-v${pkg.version}-web`;
  const dataRoot = join(root, 'web/public/data');
  const dataManifest = json(join(dataRoot, 'manifest.json'));
  const version = safeRelative(dataManifest.version);
  const expectedVersion = readFileSync(join(root, 'data/CURRENT'), 'utf8').trim();
  if (version !== expectedVersion) throw new Error('Stale game data: sync data/CURRENT before packaging');
  if (!Array.isArray(dataManifest.files) || !dataManifest.files.length
      || new Set(dataManifest.files).size !== dataManifest.files.length) throw new Error('Invalid data manifest');
  const dataFiles = {};
  for (const file of [...dataManifest.files].sort()) {
    const content = readInput(join(dataRoot, version), file).toString('utf8');
    JSON.parse(content);
    Object.defineProperty(dataFiles, file, { value: content, enumerable: true });
  }
  const data = Buffer.from(JSON.stringify({ version, files: dataFiles }));
  const compressedData = gzipSync(data, { level: 9 });
  const schemaVersion = Number(readFileSync(join(root, 'apps/pobr-wasm/src/lib.rs'), 'utf8')
    .match(/pub const SCHEMA_VERSION: u32 = (\d+);/)?.[1]);
  if (!schemaVersion) throw new Error('Missing JSON schema version');
  const baseline = json(join(root, 'devs/ci/pob-web-wasm-size-baseline.json'));
  const baselineBytes = baseline.files.reduce((sum, file) => sum + file.bytes, 0);
  const temp = mkdtempSync(join(tmpdir(), 'pobr-wasm-release-'));
  try {
    const stage = join(temp, name);
    mkdirSync(stage);
    const packageFiles = new Set([
      'pobr_wasm.js', 'pobr_wasm_bg.wasm', 'pobr_wasm.d.ts', 'pobr_wasm_bg.wasm.d.ts',
    ]);
    const runtime = [];
    for (const file of [...packageFiles].sort()) {
      const bytes = readInput(pkgRoot, file);
      mkdirSync(dirname(join(stage, file)), { recursive: true });
      writeFileSync(join(stage, file), bytes);
      if (file.endsWith('.js') || file.endsWith('.wasm')) {
        runtime.push({ name: file, bytes: bytes.length, gzipBytes: gzipSync(bytes, { level: 9 }).length, sha256: sha256(bytes) });
      }
    }
    writeFileSync(join(stage, 'pobr-data.json.gz'), compressedData);
    writeFileSync(join(stage, 'package.json'), pretty({ ...pkg, type: 'module', private: true,
      files: [...packageFiles, 'pobr-data.json.gz', 'api-types.ts', 'manifest.json', 'example.mjs', 'README.md', 'LICENSE', 'skills'] }));
    for (const [source, dest] of [
      ['web/src/api/types.ts', 'api-types.ts'], ['LICENSE', 'LICENSE'],
      ['docs/wasm-package.md', 'README.md'], ['.claude/skills/use-pobr-wasm/scripts/demo.mjs', 'example.mjs'],
    ]) cpSync(join(root, source), join(stage, dest));
    cpSync(join(root, '.claude/skills/use-pobr-wasm'), join(stage, 'skills/use-pobr-wasm'), { recursive: true });
    const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
    const manifest = { packageVersion: pkg.version, schemaVersion, dataVersion: version, commit,
      target: 'web', dataFileCount: dataManifest.files.length,
      files: [...runtime, { name: 'pobr-data.json.gz', bytes: compressedData.length,
        uncompressedBytes: data.length, sha256: sha256(compressedData) }] };
    writeFileSync(join(stage, 'manifest.json'), pretty(manifest));
    mkdirSync(outDir, { recursive: true });
    const archive = join(outDir, `${name}.tar.gz`);
    execFileSync('tar', ['-czf', archive, '-C', temp, name], { env: { ...process.env, COPYFILE_DISABLE: '1' } });
    const archiveBytes = readFileSync(archive);
    const hybridBytes = runtime.reduce((sum, file) => sum + file.bytes, compressedData.length);
    const gzipBytes = runtime.reduce((sum, file) => sum + file.gzipBytes, compressedData.length);
    const report = { ...manifest, archive: { name: `${name}.tar.gz`, bytes: archiveBytes.length, sha256: sha256(archiveBytes) },
      data: { bytes: data.length, gzipBytes: compressedData.length },
      totals: { wasmJsCompressedDataBytes: hybridBytes, gzipTransferEstimateBytes: gzipBytes },
      reference: { ...baseline, totalBytes: baselineBytes },
      ratioToApproximateReference: hybridBytes / baselineBytes };
    const markdown = [
      `# PoBR WASM v${pkg.version}`, '',
      `Data: ${version}; JSON schema: ${schemaVersion}; source commit: ${commit}.`, '',
      '| Component | Raw bytes | gzip bytes (level 9) |', '| --- | ---: | ---: |',
      ...runtime.map(file => `| ${file.name} | ${file.bytes} | ${file.gzipBytes} |`),
      `| Data bundle (${dataManifest.files.length} files) | ${data.length} | ${compressedData.length} |`, '',
      '| Disk / transfer comparison | MiB |', '| --- | ---: |',
      `| PoBR WASM + JS + compressed data | ${mib(hybridBytes)} |`,
      `| pob-web approximate reference (same compression convention) | ${mib(baselineBytes)} |`,
      `| PoBR gzip transfer estimate (WASM + JS + data) | ${mib(gzipBytes)} |`,
      `| Complete release archive (also includes types, docs, example) | ${mib(archiveBytes.length)} |`, '',
      `PoBR / approximate reference: ${(hybridBytes / baselineBytes).toFixed(2)}x.`, '',
      `Reference: [${baseline.project}](${baseline.source}). ${baseline.provenance}`, '',
      'The reference is Lua WASM + JS + compressed PoB Lua code/data; PoBR compiles calculation code into WASM.',
      'PoBR includes every JSON from the Web data manifest, including catalogs, localization and derived data.',
      'These scopes and supported mechanics differ. This is a size report, not a performance or parity gate.',
      'gzip estimates assume server compression for JS/WASM; the data file is already compressed.',
      'Disk and compressed transfer sizes do not measure runtime memory.', '',
    ].join('\n');
    writeFileSync(join(outDir, 'wasm-size.json'), pretty(report));
    writeFileSync(join(outDir, 'wasm-size.md'), markdown);
    const assets = [`${name}.tar.gz`, 'wasm-size.json', 'wasm-size.md'];
    writeFileSync(join(outDir, 'SHA256SUMS'), assets.map(file => `${sha256(readFileSync(join(outDir, file)))}  ${file}\n`).join(''));
    return { archive, report, markdown };
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const tag = process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : undefined;
  const { archive, markdown } = packageWasm({ tag });
  console.log(markdown);
  console.log(`Created ${archive}`);
}
