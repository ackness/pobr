import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { gunzipSync } from 'node:zlib';
import { packageWasm } from './package-wasm.mjs';
import { bindingHashes, compiledSchema, sourceFingerprint } from './wasm-build-inputs.mjs';
import { syncData } from './sync-data.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'pobr package test '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (file, value) => {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    writeFileSync(join(root, file), typeof value === 'string' ? value : JSON.stringify(value));
  };
  execFileSync('git', ['init', '--quiet'], { cwd: root });
  execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '--allow-empty', '-m', 'fixture'], { cwd: root });
  write('Cargo.toml', '[workspace.package]\nversion = "0.1.0"\n');
  write('apps/pobr-wasm/src/lib.rs', 'pub const SCHEMA_VERSION: u32 = 4;');
  write('web/src/wasm/pkg/package.json', { name: 'pobr-wasm', version: '0.1.0', type: 'module', files: [] });
  for (const file of ['pobr_wasm.js', 'pobr_wasm_bg.wasm', 'pobr_wasm.d.ts', 'pobr_wasm_bg.wasm.d.ts']) {
    write(`web/src/wasm/pkg/${file}`, `synthetic ${file}`);
  }
  // A real minimal module exporting schemaVersion() == 4, without a Rust build.
  writeFileSync(join(root, 'web/src/wasm/pkg/pobr_wasm_bg.wasm'), Buffer.from(
    '0061736d010000000105016000017f030201000711010d736368656d6156657273696f6e00000a0601040041040b', 'hex'));
  write('web/src/wasm/pkg/pobr_wasm.js', `let wasm;
    export function initSync({ module }) { wasm = new WebAssembly.Instance(new WebAssembly.Module(module)).exports; }
    export function schemaVersion() { return wasm.schemaVersion(); }`);
  // A package must not sweep up unrelated files from the generated directory.
  write('web/src/wasm/pkg/unrelated.txt', 'must stay local');
  write('data/CURRENT', '1.2.3\n');
  write('data/1.2.3/manifest.json', { schema_version: 2, poe_version: '1.2.3', domains: [], languages: [] });
  write('data/1.2.3/base/items.json', [{ name: '测试 item' }]);
  write('data/overlay-common/special_mods.json', { test: true });
  syncData({ dataRoot: join(root, 'data'), destRoot: join(root, 'web/public/data') });
  for (const file of ['devs/ci/pob-web-wasm-size-baseline.json', 'docs/wasm-package.md', 'LICENSE', 'web/src/api/types.ts']) {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    cpSync(join(repo, file), join(root, file));
  }
  cpSync(join(repo, '.claude/skills/use-pobr-wasm'), join(root, '.claude/skills/use-pobr-wasm'), { recursive: true });
  const pkgRoot = join(root, 'web/src/wasm/pkg');
  write('web/src/wasm/pkg/build-receipt.json', {
    version: 1, sourceFingerprint: sourceFingerprint(root), schemaVersion: compiledSchema(pkgRoot),
    commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    dirty: true, files: bindingHashes(pkgRoot),
  });
  return { root, write };
}

test('archive preserves complete data, bindings and skill; hashes and size arithmetic match bytes', t => {
  const { root } = fixture(t);
  const { archive, report } = packageWasm({ root, tag: 'v0.1.0' });
  const extracted = join(root, 'extracted');
  mkdirSync(extracted);
  execFileSync('tar', ['-xzf', archive, '-C', extracted]);
  const packageRoot = join(extracted, 'pobr-wasm-v0.1.0-web');
  const data = JSON.parse(gunzipSync(readFileSync(join(packageRoot, 'pobr-data.json.gz'))));
  assert.equal(data.version, '1.2.3');
  assert.deepEqual(Object.keys(data.files), ['base/items.json', 'manifest.json', 'overlay-common/special_mods.json']);
  for (const [file, content] of Object.entries(data.files)) {
    assert.equal(content, readFileSync(join(root, 'web/public/data/1.2.3', file), 'utf8'));
  }
  for (const file of ['pobr_wasm.d.ts', 'pobr_wasm_bg.wasm.d.ts', 'api-types.ts', 'skills/use-pobr-wasm/SKILL.md',
    'skills/use-pobr-wasm/references/api.md', 'skills/use-pobr-wasm/scripts/demo.mjs', 'example.mjs', 'LICENSE']) {
    assert.ok(readFileSync(join(packageRoot, file)).length);
  }
  assert.throws(() => readFileSync(join(packageRoot, 'unrelated.txt')), { code: 'ENOENT' });
  assert.equal(report.totals.wasmJsCompressedDataBytes, report.files.reduce((sum, file) => sum + file.bytes, 0));
  assert.equal(report.reference.totalBytes, 262144 + 69120 + 4875878);
  for (const line of readFileSync(join(root, '.cache/wasm-release/SHA256SUMS'), 'utf8').trim().split('\n')) {
    const [expected, name] = line.split('  ');
    assert.equal(createHash('sha256').update(readFileSync(join(root, '.cache/wasm-release', name))).digest('hex'), expected);
  }
});

test('reject mismatched release and data versions without changing source files', t => {
  const { root, write } = fixture(t);
  assert.throws(() => packageWasm({ root, tag: 'v0.2.0' }), /does not match/);
  write('web/src/wasm/pkg/package.json', { version: '0.0.9' });
  assert.throws(() => packageWasm({ root }), /Stale WASM/);
  write('web/src/wasm/pkg/package.json', { version: '0.1.0' });
  write('data/CURRENT', 'new-data\n');
  assert.throws(() => packageWasm({ root }), /Stale game data/);
  assert.equal(readFileSync(join(root, 'data/CURRENT'), 'utf8'), 'new-data\n');
});

test('reject missing files and unsafe data paths instead of shipping a partial bundle', t => {
  const { root, write } = fixture(t);
  for (const file of ['base/missing.json', '../outside.json', '/absolute.json']) {
    write('web/public/data/manifest.json', { version: '1.2.3', files: [file] });
    assert.throws(() => packageWasm({ root }), /ENOENT|Unsafe package path/);
  }
  write('web/public/data/manifest.json', { version: '1.2.3', files: ['base/link.json'] });
  symlinkSync(join(root, 'data/CURRENT'), join(root, 'web/public/data/1.2.3/base/link.json'));
  assert.throws(() => packageWasm({ root }), /Symlink/);
});

test('reject same-version stale data and newly added patches until synchronized', t => {
  const { root, write } = fixture(t);
  write('data/1.2.3/base/items.json', [{ name: 'new data' }]);
  assert.throws(() => packageWasm({ root }), /Stale game data snapshot/);
  syncData({ dataRoot: join(root, 'data'), destRoot: join(root, 'web/public/data') });
  packageWasm({ root });
  write('data/1.2.3/patch/base/items.json', [{ name: 'patched' }]);
  assert.throws(() => packageWasm({ root }), /Stale game data snapshot/);
});

test('reject same-version source and binding changes, including new embedded inputs', t => {
  const { root, write } = fixture(t);
  write('apps/pobr-wasm/src/lib.rs', 'pub const SCHEMA_VERSION: u32 = 5;');
  assert.throws(() => packageWasm({ root }), /Stale WASM source snapshot/);
  write('apps/pobr-wasm/src/lib.rs', 'pub const SCHEMA_VERSION: u32 = 4;');
  write('crates/pobr-i18n/locales/en-US/ui.toml', 'new = "embedded"');
  assert.throws(() => packageWasm({ root }), /Stale WASM source snapshot/);
  rmSync(join(root, 'crates'), { recursive: true });
  write('web/src/wasm/pkg/pobr_wasm.d.ts', 'changed binding');
  assert.throws(() => packageWasm({ root }), /bindings differ/);
});

test('read the schema from compiled WASM and retain its build commit after unrelated commits', t => {
  const { root, write } = fixture(t);
  const receiptPath = join(root, 'web/src/wasm/pkg/build-receipt.json');
  const receipt = JSON.parse(readFileSync(receiptPath, 'utf8'));
  execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '--allow-empty', '-m', 'unrelated docs'], { cwd: root });
  const { report } = packageWasm({ root });
  assert.equal(report.commit, receipt.commit);
  assert.equal(report.schemaVersion, 4);
  write('web/src/wasm/pkg/build-receipt.json', { ...receipt, schemaVersion: 5 });
  assert.throws(() => packageWasm({ root }), /compiled schema differs/);
  rmSync(receiptPath);
  assert.throws(() => packageWasm({ root }), /Missing WASM build receipt/);
});

test('build wrapper invalidates receipts on failure or concurrent edits and seals successful output', t => {
  const { root, write } = fixture(t);
  for (const file of ['build-wasm.mjs', 'wasm-build-inputs.mjs']) {
    mkdirSync(join(root, 'web/scripts'), { recursive: true });
    cpSync(join(repo, 'web/scripts', file), join(root, 'web/scripts', file));
  }
  const wrapper = join(root, 'web/scripts/build-wasm.mjs');
  const receipt = join(root, 'web/src/wasm/pkg/build-receipt.json');
  const env = { ...process.env, PATH: `${join(root, 'bin')}${delimiter}${process.env.PATH}` };
  const tool = script => {
    write('bin/wasm-pack', `#!/bin/sh\n${script}\n`);
    chmodSync(join(root, 'bin/wasm-pack'), 0o755);
  };
  const run = () => execFileSync(process.execPath, [wrapper], { cwd: root, env, stdio: 'pipe' });
  tool('exit 17');
  assert.throws(run);
  assert.equal(existsSync(receipt), false);
  tool('echo "// concurrent source edit" >> apps/pobr-wasm/src/lib.rs');
  assert.throws(run, /sources changed during compilation/);
  assert.equal(existsSync(receipt), false);
  tool('if [ "$1" = "--version" ]; then echo "wasm-pack fixture"; fi');
  write('bin/rustc', '#!/bin/sh\necho "rustc fixture"\n');
  chmodSync(join(root, 'bin/rustc'), 0o755);
  run();
  const { report } = packageWasm({ root });
  assert.equal(report.schemaVersion, 4);
  assert.equal(report.build.rustc, 'rustc fixture');
  assert.equal(report.build.wasmPack, 'wasm-pack fixture');
});
