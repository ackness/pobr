// These fixtures exercise the real CLI/loading boundary with a SYNTHETIC JS
// calculator and a tiny schema-only WASM. They are not real PoBR WASM evidence.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { gzipSync } from 'node:zlib';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const script = join(repo, 'web/scripts/smoke-runtime-candidate.mjs');
const schema = Number(readFileSync(join(repo, 'web/src/api/types.ts'), 'utf8').match(/EXPECTED_SCHEMA_VERSION = (\d+)/)[1]);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const encode = value => JSON.stringify(value, (_, item) => item && typeof item === 'object' && !Array.isArray(item)
  ? Object.fromEntries(Object.entries(item).sort()) : item);
const version = '9.1';
const runtime = hash('SYNTHETIC candidate runtime; not game data');
const packageName = 'pobr-wasm-v0.0.0-synthetic-web';
const worked = ['monk-invoker-frost-bomb', 'mercenary-tactician-wolf-pack', 'ranger-deadeye-explosive-grenade'];
const sentinel = 'POBR_SYNTHETIC_RUNTIME_CANDIDATE_UNSUPPORTED_7B21A6CF_DO_NOT_MODEL';
const codes = worked.map(name => readFileSync(join(repo, `examples/demo-bd-test/builds/${name}/code.txt`), 'utf8').trim());

// Minimal ustar writer keeps fixtures tiny and can represent deliberately unsafe
// entries without ever writing those paths to disk.
function archive(entries) {
  const blocks = [];
  for (const { name, bytes = Buffer.alloc(0), type = '0', link = '' } of entries) {
    const header = Buffer.alloc(512);
    const text = (value, start, length) => header.write(value, start, length, 'utf8');
    const octal = (value, start, length) => text(value.toString(8).padStart(length - 1, '0') + '\0', start, length);
    text(name, 0, 100);
    octal(0o644, 100, 8);
    octal(0, 108, 8);
    octal(0, 116, 8);
    octal(bytes.length, 124, 12);
    octal(0, 136, 12);
    header.fill(32, 148, 156);
    text(type, 156, 1);
    text(link, 157, 100);
    text('ustar\0', 257, 6);
    text('00', 263, 2);
    const checksum = header.reduce((sum, byte) => sum + byte, 0);
    text(checksum.toString(8).padStart(6, '0') + '\0 ', 148, 8);
    blocks.push(header, bytes, Buffer.alloc((512 - bytes.length % 512) % 512));
  }
  return gzipSync(Buffer.concat([...blocks, Buffer.alloc(1024)]));
}

function fixture(t, behavior = {}) {
  const root = mkdtempSync(join(realpathSync(tmpdir()), 'pobr candidate synthetic '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const packageRoot = join(root, 'standalone');
  const data = join(root, 'materialized');
  const out = join(root, 'receipt.json');
  const trace = join(root, 'synthetic-trace.jsonl');
  const write = (path, bytes) => {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, bytes);
  };
  const json = (path, value) => write(path, JSON.stringify(value));
  const staged = {
    'base/ordinary.json': JSON.stringify({ synthetic: 'NEW ordinary candidate' }),
    'generated/display.json': JSON.stringify({ synthetic: 'NEW display candidate' }),
    'i18n/zh-CN/labels.json': JSON.stringify({ synthetic: '新候选' }),
    'overlay-common/special_mods.json': JSON.stringify({ synthetic: 'NEW shared candidate' }),
  };
  const manifest = { schema_version: 3, poe_version: version, domains: { base: ['ordinary'], generated: ['display'] },
    languages: ['zh-CN'], files: Object.fromEntries(Object.entries(staged)
      .filter(([name]) => !name.startsWith('overlay-common/')).map(([name, value]) => [name, hash(value)])) };
  staged['manifest.json'] = JSON.stringify(manifest);
  for (const [name, bytes] of Object.entries(staged)) write(join(data, 'data', name.startsWith('overlay-common/') ? name : `${version}/${name}`), bytes);
  write(join(data, 'data/CURRENT'), `${version}\n`);
  const bundle = { schema: 'pobr-data-bundle/v1', data_version: version, required_manifest_schema: 3, runtime_identity: runtime,
    files: { 'data/CURRENT': hash(`${version}\n`), ...Object.fromEntries(Object.entries(staged)
      .map(([name, bytes]) => [`data/${name.startsWith('overlay-common/') ? name : `${version}/${name}`}`, hash(bytes)])) },
    verification: 'runtime byte integrity; not client-source or game-semantic certification',
    user_patch: 'excluded; apply and validate separately' };
  const sealBundle = () => {
    delete bundle.bundle_id;
    bundle.bundle_id = hash(encode(bundle));
    json(join(data, 'bundle.json'), bundle);
  };
  sealBundle();
  // A real minimal module exporting only schemaVersion(); there is no PoBR
  // calculation inside these bytes. All result values below are synthetic.
  const wasm = Buffer.from(`0061736d010000000105016000017f030201000711010d736368656d6156657273696f6e00000a0601040041${(behavior.compiledSchema ?? schema).toString(16).padStart(2, '0')}0b`, 'hex');
  const js = `// SYNTHETIC calculator: not a PoBR engine or numerical regression test.
import assert from 'node:assert/strict';
import { appendFileSync, existsSync, writeFileSync, renameSync, symlinkSync } from 'node:fs';
const expected = ${JSON.stringify(staged)};
const behavior = ${JSON.stringify(behavior)};
const files = {};
let compiled, ready = false;
const trace = event => appendFileSync(process.env.SYNTHETIC_SMOKE_TRACE, JSON.stringify(event) + '\\n');
export function initSync({ module }) {
  assert.deepEqual(Buffer.from(module), Buffer.from(${JSON.stringify(wasm.toString('hex'))}, 'hex'));
  compiled = new WebAssembly.Instance(new WebAssembly.Module(module)).exports;
  assert.equal(existsSync(new URL('./pobr-data.json.gz', import.meta.url)), false, 'old packaged data must not be extracted');
  trace({ call: 'initSync', synthetic: true });
}
export function schemaVersion() { return compiled.schemaVersion(); }
export function isDataReady() { return behavior.alreadyReady || ready; }
export function stageDataFile(path, content) {
  assert.equal(ready, false);
  assert.ok(Object.hasOwn(expected, path), 'unexpected staged input');
  assert.equal(content, expected[path], 'must stage candidate bytes');
  files[path] = content;
  trace({ call: 'stageDataFile', path, content });
}
export function initStagedData() {
  assert.deepEqual(files, expected, 'all candidate layers must be staged');
  if (behavior.initError) throw new Error('synthetic init error');
  ready = !behavior.notReady;
  trace({ call: 'initStagedData' });
}
export function calculateBuildJson(raw) {
  assert.equal(ready, true);
  const wire = JSON.parse(raw);
  const reopened = wire.pob_code?.startsWith('SYNTHETIC_SAVED:');
  const request = reopened ? JSON.parse(Buffer.from(wire.pob_code.slice('SYNTHETIC_SAVED:'.length), 'base64url')) : wire;
  if (!reopened && request.pob_code) {
    assert.ok(${JSON.stringify(codes)}.includes(request.pob_code), 'must use existing worked build code');
    assert.equal(request.full_dps, true);
    assert.deepEqual(request.breakdowns, []);
  } else {
    assert.deepEqual(request.character, { class_name: 'Warrior', level: 90 });
    assert.deepEqual(request.config_inputs, { 'questInterlude 2Khari CrossingMolten Shrine': false });
  }
  trace({ call: 'calculateBuildJson', request: wire });
  if (behavior.mutate) writeFileSync(process.env.SYNTHETIC_MUTATE_PATH, 'changed while calculating');
  if (behavior.addInput) writeFileSync(process.env.SYNTHETIC_MUTATE_PATH, '{}');
  if (behavior.swapLink) {
    const path = process.env.SYNTHETIC_MUTATE_PATH;
    if (!existsSync(path + '.original')) { renameSync(path, path + '.original'); symlinkSync(path + '.original', path); }
  }
  if (behavior.concurrentReceipt) writeFileSync(process.env.SYNTHETIC_RECEIPT_PATH, 'other writer');
  if (behavior.calculateError) throw new Error('synthetic calculation error');
  const result = {
    provenance: { engine_version: '0.0.0-synthetic', data_version: behavior.wrongVersion ? '0.0' : '${version}',
      data_sha256: behavior.wrongRuntime ? '${'e'.repeat(64)}' : '${runtime}' },
    stats: [{ id: 'Life', value: behavior.badLife ? null : 1234 + (request.extra_modifiers?.includes('+100 to maximum Life') ? (behavior.wrongEdit ? 99 : 100) : 0) }],
    main_skill: null, unsupported_modifiers: ['SYNTHETIC unsupported line'],
    item_errors: [{ slot: 'synthetic-slot', error: 'SYNTHETIC item diagnostic' }],
    missing_items: ['SYNTHETIC missing item'],
    tree_effects: { warnings: ['SYNTHETIC tree diagnostic'] },
    full_dps: { dps: 0, dot_dps: 0, skills: [{ unsupported_modifiers: ['SYNTHETIC skill diagnostic'] }] },
  };
  if (request.extra_modifiers?.includes('${sentinel}') && !behavior.dropSentinel) {
    result.unsupported_modifiers.push(reopened && !behavior.stripSource ? 'Config: ${sentinel}' : '${sentinel}');
  }
  if (reopened && behavior.reopenItemErrors) result.item_errors = [];
  if (behavior.dropDiagnostics) delete result[behavior.dropDiagnostics];
  return JSON.stringify(result);
}
export function encodeBuildJson(raw) {
  const request = JSON.parse(raw);
  trace({ call: 'encodeBuildJson', request });
  if (behavior.encodeError) throw new Error('synthetic save error');
  if (behavior.saveDropsSentinel) request.extra_modifiers = request.extra_modifiers.filter(line => line !== '${sentinel}');
  return 'SYNTHETIC_SAVED:' + Buffer.from(JSON.stringify(request)).toString('base64url');
}`;
  const packageManifest = { packageVersion: '0.0.0-synthetic', schemaVersion: schema,
    dataVersion: '0.0.1', commit: 'a'.repeat(40), target: 'web', dataFileCount: 1,
    build: { commit: 'a'.repeat(40), sourceFingerprint: 'b'.repeat(64), schemaVersion: schema, dirty: false,
      files: { 'pobr_wasm.js': hash(js), 'pobr_wasm_bg.wasm': hash(wasm) } },
    files: [
      { name: 'pobr_wasm.js', bytes: Buffer.byteLength(js), sha256: hash(js) },
      { name: 'pobr_wasm_bg.wasm', bytes: wasm.length, sha256: hash(wasm) },
      // Intentionally not a gzip: any attempt to decode/stage old packaged data fails.
      { name: 'pobr-data.json.gz', bytes: 8, sha256: hash('OLD DATA') },
    ] };
  const members = new Map([
    ['manifest.json', Buffer.from(JSON.stringify(packageManifest))], ['pobr_wasm.js', Buffer.from(js)],
    ['pobr_wasm_bg.wasm', wasm], ['api-types.ts', Buffer.from(`export const EXPECTED_SCHEMA_VERSION = ${schema};`)],
    ['pobr-data.json.gz', Buffer.from('OLD DATA')],
  ]);
  let extraEntries = [];
  const report = { ...packageManifest };
  const sealPackage = () => {
    const bytes = archive([...members].map(([name, bytes]) => ({ name: `${packageName}/${name}`, bytes })).concat(extraEntries));
    const name = `${packageName}.tar.gz`;
    report.archive = { name, bytes: bytes.length, sha256: hash(bytes) };
    write(join(packageRoot, name), bytes);
    json(join(packageRoot, 'wasm-size.json'), report);
    write(join(packageRoot, 'SHA256SUMS'), [name, 'wasm-size.json'].map(name => `${hash(readFileSync(join(packageRoot, name)))}  ${name}\n`).join(''));
  };
  sealPackage();
  return { root, packageRoot, data, out, trace, write, json, bundle, sealBundle, members, report,
    sealPackage, extra: entries => { extraEntries = entries; },
    run(options = {}) {
      return spawnSync(process.execPath, [script, '--package', packageRoot, '--data', data,
        '--expected-runtime', runtime, '--out', out, ...(options.args ?? [])], {
        encoding: 'utf8', timeout: 20_000,
        env: { ...process.env, POBR_DATA_VERSION: 'poison', POBR_DATA_ROOT: '/missing-repo-data',
          POBR_RUNTIME_PACK: '/missing-compact', POBR_RUNTIME_RELEASE_ID: 'poison',
          SYNTHETIC_SMOKE_TRACE: trace, SYNTHETIC_RECEIPT_PATH: out, ...options.env },
      });
    },
  };
}

function rejected(f, pattern, options) {
  const result = f.run(options);
  assert.equal(result.status, 1, result.stderr || result.error?.message);
  assert.match(result.stderr, pattern);
  assert.equal(existsSync(f.out), false, 'failure must not publish a success receipt');
  return result;
}

test('synthetic CLI consumes only explicit candidate layers, real worked inputs and the existing edit probe', t => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  const receipt = JSON.parse(readFileSync(f.out, 'utf8'));
  assert.equal(receipt.schema, 'pobr-runtime-candidate-smoke/v1');
  assert.equal(receipt.status, 'passed');
  assert.equal(receipt.package.package_version, '0.0.0-synthetic');
  assert.equal(receipt.package.bundled_data_used, false);
  assert.equal(receipt.candidate.bundle_id, f.bundle.bundle_id);
  assert.equal(receipt.candidate.runtime_identity, runtime);
  assert.equal(receipt.candidate.data_version, version);
  assert.equal(receipt.candidate.staged_files.length, 5);
  assert.equal(receipt.calculations.length, 6);
  assert.deepEqual(receipt.calculations.slice(0, 3).map(row => row.name), worked);
  assert.deepEqual(receipt.edit_check, { modifier: '+100 to maximum Life', life_delta: 100 });
  assert.equal(receipt.diagnostic_check.synthetic_modifier, sentinel);
  assert.deepEqual(receipt.diagnostic_check.direct, { source: 'extra_modifiers', expected_diagnostic: sentinel });
  assert.deepEqual(receipt.diagnostic_check.reopened, { source: 'Config', expected_diagnostic: `Config: ${sentinel}` });
  assert.deepEqual(receipt.diagnostic_check.save_reopen_preserved, ['unsupported_modifiers (with Config source)', 'item_errors', 'Life']);
  assert.match(receipt.code.head, /^[a-f0-9]{40}$/);
  assert.match(receipt.scope, /no numerical golden/);
  for (const [index, row] of receipt.calculations.entries()) {
    assert.equal(row.result.provenance.data_sha256, runtime);
    assert.deepEqual(row.result.unsupported_modifiers, ['SYNTHETIC unsupported line', ...(index >= 3 ? [index === 5 ? `Config: ${sentinel}` : sentinel] : [])]);
    assert.equal(row.result.item_errors[0].error, 'SYNTHETIC item diagnostic');
    assert.deepEqual(row.result.missing_items, ['SYNTHETIC missing item']);
    assert.deepEqual(row.result.tree_effects.warnings, ['SYNTHETIC tree diagnostic']);
    assert.deepEqual(row.result.full_dps.skills[0].unsupported_modifiers, ['SYNTHETIC skill diagnostic']);
    assert.match(row.request_sha256, /^[a-f0-9]{64}$/);
  }
  const calls = readFileSync(f.trace, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls.map(row => row.call), ['initSync', ...Array(5).fill('stageDataFile'), 'initStagedData',
    ...Array(5).fill('calculateBuildJson'), 'encodeBuildJson', 'calculateBuildJson']);
  assert.deepEqual(calls.filter(row => row.call === 'calculateBuildJson').slice(0, 3).map(row => row.request.pob_code), codes);
  assert.deepEqual(calls.find(row => row.call === 'encodeBuildJson').request.extra_modifiers, [sentinel, '+100 to maximum Life']);
  assert.ok(calls.at(-1).request.pob_code.startsWith('SYNTHETIC_SAVED:'));
});

test('synthetic missing or corrupted candidate fails before any package engine invocation', async t => {
  for (const kind of ['missing', 'snapshot', 'shared', 'bundle-id', 'bundle-file-hash', 'current', 'extra', 'unsafe-version', 'unsafe-inventory', 'schema']) {
    await t.test(kind, t => {
      const f = fixture(t);
      if (kind === 'missing') renameSync(f.data, `${f.data}-absent`);
      if (kind === 'snapshot') f.write(join(f.data, `data/${version}/base/ordinary.json`), '{}');
      if (kind === 'shared') f.write(join(f.data, 'data/overlay-common/special_mods.json'), '{}');
      if (kind === 'bundle-id') { f.bundle.bundle_id = '0'.repeat(64); f.json(join(f.data, 'bundle.json'), f.bundle); }
      if (kind === 'bundle-file-hash') { f.bundle.files['data/CURRENT'] = '0'.repeat(64); f.sealBundle(); }
      if (kind === 'current') f.write(join(f.data, 'data/CURRENT'), '0.0\n');
      if (kind === 'extra') f.write(join(f.data, `data/${version}/generated/unlisted.json`), '{}');
      if (kind === 'unsafe-version') { f.bundle.data_version = '../../escape'; f.sealBundle(); }
      if (kind === 'unsafe-inventory') { f.bundle.files['../escape'] = '0'.repeat(64); f.sealBundle(); }
      if (kind === 'schema') f.json(join(f.data, `data/${version}/manifest.json`), { schema_version: 2 });
      rejected(f, /ENOENT|fingerprint mismatch|hash mismatch|identity mismatch|CURRENT\/version|unlisted|Invalid candidate|input set mismatch|schema 3/);
      assert.equal(existsSync(f.trace), false);
    });
  }
});

test('synthetic CLI requires every explicit argument and a matching runtime digest', async t => {
  for (const args of [['--expected-runtime', 'bad'], ['--expected-runtime', 'f'.repeat(64)], ['--data', 'missing'], ['--unknown', 'x']]) {
    await t.test(args.join(' '), t => rejected(fixture(t), /SHA256|runtime identity|ENOENT|Unknown option/, { args }));
  }
  const result = spawnSync(process.execPath, [script], { encoding: 'utf8' });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /expected-runtime/);
});

test('synthetic package integrity and all three schema boundaries fail closed', async t => {
  for (const kind of ['asset-hash', 'archive-size', 'runtime-js', 'runtime-wasm', 'manifest-report', 'build-hash', 'contract-schema', 'report-schema', 'compiled-schema', 'missing-js']) {
    await t.test(kind, t => {
      const f = fixture(t, kind === 'compiled-schema' ? { compiledSchema: schema - 1 } : {});
      if (kind === 'asset-hash') f.write(join(f.packageRoot, `${packageName}.tar.gz`), 'tampered');
      else {
        if (kind === 'runtime-js' || kind === 'runtime-wasm') {
          const name = kind === 'runtime-js' ? 'pobr_wasm.js' : 'pobr_wasm_bg.wasm';
          const bytes = Buffer.from(f.members.get(name)); bytes[0] ^= 1; f.members.set(name, bytes);
        }
        if (kind === 'manifest-report') { const m = JSON.parse(f.members.get('manifest.json')); m.commit = 'c'.repeat(40); f.members.set('manifest.json', Buffer.from(JSON.stringify(m))); }
        if (kind === 'build-hash') {
          f.report.build.files['pobr_wasm.js'] = 'c'.repeat(64);
          const { archive, ...manifest } = f.report; f.members.set('manifest.json', Buffer.from(JSON.stringify(manifest)));
        }
        if (kind === 'contract-schema') f.members.set('api-types.ts', Buffer.from('export const EXPECTED_SCHEMA_VERSION = 0;'));
        if (kind === 'report-schema') {
          f.report.schemaVersion = 0;
          const { archive, ...manifest } = f.report; f.members.set('manifest.json', Buffer.from(JSON.stringify(manifest)));
        }
        if (kind === 'missing-js') f.members.delete('pobr_wasm.js');
        if (kind === 'archive-size') {
          f.report.archive.bytes++;
          f.json(join(f.packageRoot, 'wasm-size.json'), f.report);
          const name = `${packageName}.tar.gz`;
          f.write(join(f.packageRoot, 'SHA256SUMS'), [name, 'wasm-size.json'].map(name => `${hash(readFileSync(join(f.packageRoot, name)))}  ${name}\n`).join(''));
        } else f.sealPackage();
      }
      rejected(f, /hash mismatch|size mismatch|manifest\/report mismatch|build\/runtime mismatch|schema mismatch|Missing package member/);
    });
  }
});

test('synthetic archives cannot escape or use links, duplicate names or special entries', async t => {
  for (const entry of [
    { name: '../escaped' }, { name: '/escaped' }, { name: `${packageName}/../escaped` },
    { name: `${packageName}/pobr_wasm.js` },
    { name: `${packageName}/link`, type: '2', link: '/outside' },
    { name: `${packageName}/hard`, type: '1', link: `${packageName}/pobr_wasm.js` },
    { name: `${packageName}/fifo`, type: '6' },
  ]) {
    await t.test(`${entry.name} type ${entry.type ?? '0'}`, t => {
      const f = fixture(t);
      f.extra([entry]); f.sealPackage();
      rejected(f, /Unsafe input path|Unsafe archive member|Duplicate archive member|links\/special/);
      assert.equal(existsSync(join(f.root, 'escaped')), false);
    });
  }
});

test('synthetic paths reject symlinks, parent traversal and receipt overwrite', async t => {
  for (const kind of ['data-root', 'candidate-file', 'candidate-directory', 'package-asset', 'package-root', 'output-parent', 'output-leaf', 'parent-data', 'parent-out', 'inside-data', 'existing-out']) {
    await t.test(kind, t => {
      const f = fixture(t);
      let args;
      if (['data-root', 'package-root', 'output-parent'].includes(kind)) {
        const link = join(f.root, 'link');
        symlinkSync(kind === 'data-root' ? f.data : kind === 'package-root' ? f.packageRoot : f.root, link);
        args = kind === 'data-root' ? ['--data', link] : kind === 'package-root' ? ['--package', link] : ['--out', join(link, 'new.json')];
      }
      if (['candidate-file', 'candidate-directory', 'package-asset'].includes(kind)) {
        const path = kind === 'candidate-file' ? join(f.data, 'data/CURRENT') : kind === 'candidate-directory'
          ? join(f.data, `data/${version}/base`) : join(f.packageRoot, `${packageName}.tar.gz`);
        renameSync(path, `${path}-original`); symlinkSync(`${path}-original`, path);
      }
      if (kind === 'parent-data') args = ['--data', `${f.root}/unused/../materialized`];
      if (kind === 'parent-out') args = ['--out', `${f.root}/unused/../new.json`];
      if (kind === 'inside-data') args = ['--out', join(f.data, 'receipt.json')];
      if (kind === 'output-leaf') { f.write(join(f.root, 'existing'), 'preserved'); symlinkSync(join(f.root, 'existing'), f.out); }
      if (kind === 'existing-out') f.write(f.out, 'preserved');
      const result = f.run({ args });
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, /Symlink|parent directories|outside input|fresh/);
      if (['existing-out', 'output-leaf'].includes(kind)) assert.equal(readFileSync(f.out, 'utf8'), 'preserved');
      else assert.equal(existsSync(f.out), false);
      assert.equal(existsSync(f.trace), false);
    });
  }
});

test('synthetic engine errors, wrong provenance, dropped diagnostics and edit mismatch cannot pass', async t => {
  for (const behavior of [{ alreadyReady: true }, { initError: true }, { notReady: true }, { calculateError: true },
    { wrongRuntime: true }, { wrongVersion: true }, { dropDiagnostics: 'unsupported_modifiers' },
    { dropDiagnostics: 'item_errors' }, { badLife: true }, { wrongEdit: true },
    { dropSentinel: true }, { stripSource: true }, { encodeError: true }, { saveDropsSentinel: true }, { reopenItemErrors: true }]) {
    await t.test(JSON.stringify(behavior), t => {
      rejected(fixture(t, behavior), /start without|synthetic init error|initialization failed|synthetic calculation error|provenance mismatch|data version mismatch|dropped|finite Life|edit probe|sentinel lost|synthetic save error|Save\/reopen changed/);
    });
  }
});

test('synthetic changes during calculation invalidate the run even though frozen candidate bytes were staged', async t => {
  for (const kind of ['candidate', 'package', 'new-input', 'symlink']) {
    await t.test(kind, t => {
      const f = fixture(t, kind === 'new-input' ? { addInput: true } : kind === 'symlink' ? { swapLink: true } : { mutate: true });
      const path = kind === 'package' ? join(f.packageRoot, `${packageName}.tar.gz`) : kind === 'new-input'
        ? join(f.data, `data/${version}/generated/added.json`) : join(f.data, `data/${version}/base/ordinary.json`);
      rejected(f, /Input changed during smoke|input set changed during smoke|Symlink/, { env: { SYNTHETIC_MUTATE_PATH: path } });
      assert.ok(readFileSync(f.trace, 'utf8').includes('calculateBuildJson'));
    });
  }
});

test('synthetic concurrent receipt creation is preserved rather than overwritten', t => {
  const f = fixture(t, { concurrentReceipt: true });
  const result = f.run();
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /fresh/);
  assert.equal(readFileSync(f.out, 'utf8'), 'other writer');
});
