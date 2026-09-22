import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { gunzipSync } from 'node:zlib';
import { packageWasm } from './package-wasm.mjs';

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
  // A package must not sweep up unrelated files from the generated directory.
  write('web/src/wasm/pkg/unrelated.txt', 'must stay local');
  write('data/CURRENT', 'test-data\n');
  write('web/public/data/manifest.json', { version: 'test-data', files: ['base/items.json', 'overlay-common/special_mods.json'] });
  write('web/public/data/test-data/base/items.json', [{ name: '测试 item' }]);
  write('web/public/data/test-data/overlay-common/special_mods.json', { test: true });
  for (const file of ['devs/ci/pob-web-wasm-size-baseline.json', 'docs/wasm-package.md', 'LICENSE', 'web/src/api/types.ts']) {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    cpSync(join(repo, file), join(root, file));
  }
  cpSync(join(repo, '.claude/skills/use-pobr-wasm'), join(root, '.claude/skills/use-pobr-wasm'), { recursive: true });
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
  assert.equal(data.version, 'test-data');
  assert.deepEqual(Object.keys(data.files), ['base/items.json', 'overlay-common/special_mods.json']);
  for (const [file, content] of Object.entries(data.files)) {
    assert.equal(content, readFileSync(join(root, 'web/public/data/test-data', file), 'utf8'));
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
    write('web/public/data/manifest.json', { version: 'test-data', files: [file] });
    assert.throws(() => packageWasm({ root }), /ENOENT|Unsafe package path/);
  }
  write('web/public/data/manifest.json', { version: 'test-data', files: ['base/link.json'] });
  symlinkSync(join(root, 'data/CURRENT'), join(root, 'web/public/data/test-data/base/link.json'));
  assert.throws(() => packageWasm({ root }), /Symlink/);
});
