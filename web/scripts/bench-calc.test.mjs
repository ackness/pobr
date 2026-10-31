// Synthetic inputs exercise benchmark rejection and evidence preservation only.
// Real startup/calculation measurements require the separately prepared WASM/data.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { bindingFiles } from './wasm-build-inputs.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function harness(t) {
  const root = mkdtempSync(join(tmpdir(), 'pobr-benchmark-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, 'web/scripts');
  const data = join(root, 'web/public/data');
  const pkg = join(root, 'web/src/wasm/pkg');
  for (const directory of [scripts, join(data, 'synthetic'), pkg]) mkdirSync(directory, { recursive: true });
  for (const file of ['bench-calc.mjs', 'wasm-build-inputs.mjs']) {
    copyFileSync(new URL(file, import.meta.url), join(scripts, file));
  }
  const manifest = { version: 'synthetic', files: ['base.json'], digests: { 'base.json': hash('{}') } };
  writeFileSync(join(data, 'synthetic/base.json'), '{}');
  writeFileSync(join(data, 'manifest.json'), JSON.stringify(manifest));
  const bodies = Object.fromEntries(bindingFiles.map(file => [file, 'synthetic binding']));
  bodies['package.json'] = '{"type":"module"}';
  bodies['pobr_wasm.js'] = 'throw new Error("MUST_NOT_IMPORT_UNVERIFIED_MODULE");';
  for (const [file, body] of Object.entries(bodies)) writeFileSync(join(pkg, file), body);
  const receipt = { version: 1, schemaVersion: 16, files: Object.fromEntries(Object.entries(bodies).map(([file, body]) => [file, hash(body)])) };
  writeFileSync(join(pkg, 'build-receipt.json'), JSON.stringify(receipt));
  const output = join(root, 'observation.json');
  const run = (args = []) => spawnSync(process.execPath, [join(scripts, 'bench-calc.mjs'), '--output', output, ...args], {
    encoding: 'utf8', timeout: 5000,
  });
  return { root, data, pkg, manifest, output, run };
}

function rejected(result, expected) {
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, expected);
  assert.doesNotMatch(result.stderr, /MUST_NOT_IMPORT_UNVERIFIED_MODULE/);
}

test('benchmark rejects missing, unknown and duplicate arguments before loading artifacts', t => {
  const { run } = harness(t);
  for (const args of [['--iterations'], ['--unknown', 'x'], ['--iterations', '3', '--iterations', '4'], ['--iterations', '2']]) {
    rejected(run(args), /Missing value|Unknown or duplicate|Expected 3\.\.100/);
  }
});

test('existing observations are preserved without loading artifacts', t => {
  const { run, output } = harness(t);
  writeFileSync(output, 'original evidence');
  rejected(run(), /Refusing to overwrite an observation/);
  assert.equal(readFileSync(output, 'utf8'), 'original evidence');
});

test('data corruption fails before any engine import or report publication', t => {
  const { run, data, output } = harness(t);
  writeFileSync(join(data, 'synthetic/base.json'), '{"changed":true}');
  rejected(run(), /Data manifest hash mismatch/);
  assert.equal(existsSync(output), false);
});

test('unsafe or duplicate manifest paths fail before engine import', t => {
  const { run, data, manifest, output } = harness(t);
  for (const files of [['../outside.json'], ['/absolute.json'], ['base.json', 'base.json']]) {
    writeFileSync(join(data, 'manifest.json'), JSON.stringify({ ...manifest, files }));
    rejected(run(), /Unsafe benchmark data path|Invalid benchmark data manifest/);
    assert.equal(existsSync(output), false);
  }
});

test('bindings differing from the build receipt are rejected before JavaScript executes', t => {
  const { run, pkg, output } = harness(t);
  writeFileSync(join(pkg, 'pobr_wasm_bg.wasm'), 'changed binary');
  rejected(run(), /WASM build receipt mismatch/);
  assert.equal(existsSync(output), false);
});
