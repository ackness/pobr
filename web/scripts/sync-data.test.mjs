import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { syncData } from './sync-data.mjs';
import { runtimePackFixture } from './runtime-pack-fixture.mjs';

function fixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'pobr-sync-data-')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (name, content) => {
    const path = join(root, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, typeof content === 'string' ? content : JSON.stringify(content));
  };
  const content = '[{"id":"example"}]';
  write('source/CURRENT', '9.1\n');
  write('source/9.1/base/skill_gems.json', content);
  write('source/9.1/generated/parsed_mods.json', '{}');
  write('source/overlay-common/special_mods.json', '{}');
  write('source/9.1/patch/base/skill_gems.json', '[]');
  const manifest = { schema_version: 3, poe_version: '9.1', languages: [], domains: { base: ['skill_gems'] }, files: {
    'base/skill_gems.json': createHash('sha256').update(content).digest('hex'),
  } };
  write('source/9.1/manifest.json', manifest);
  write('public/data/previous.json', '{"keep":true}');
  return { root, write, manifest, options: { dataRoot: join(root, 'source'), destRoot: join(root, 'public/data') } };
}

test('sync uses sealed runtime files and preserves patch/common layers', (t) => {
  const { root, write, options } = fixture(t);
  write('source/9.1/overlay/gem_quality_stats.json', { effects: [{ effect_id: 'Unlisted', stats: [] }] });
  write('source/9.1/base/passive_trees/unlisted.json', []);
  const result = syncData(options);
  assert.deepEqual(result.files, ['base/skill_gems.json', 'manifest.json', 'overlay-common/special_mods.json', 'patch/base/skill_gems.json']);
  assert.equal(existsSync(join(root, 'public/data/9.1/generated/parsed_mods.json')), false);
  assert.equal(existsSync(join(root, 'public/data/9.1/overlay/gem_quality_stats.json')), false);
  assert.equal(existsSync(join(root, 'public/data/9.1/base/passive_trees/unlisted.json')), false);
  assert.equal(existsSync(join(root, 'public/data/previous.json')), false);
  assert.deepEqual(JSON.parse(readFileSync(join(root, 'public/data/manifest.json'))), result);
  // Every published file has the digest of the bytes that were published.
  assert.deepEqual(Object.keys(result.digests), result.files);
  for (const rel of result.files) {
    const published = readFileSync(join(root, 'public/data/9.1', rel));
    assert.equal(result.digests[rel], createHash('sha256').update(published).digest('hex'), rel);
    // ...and the same bytes are found under a path that names them, which is what is kept for good.
    const addressed = readFileSync(join(root, 'public/data/h', result.digests[rel].slice(0, 16), rel));
    assert.deepEqual(addressed, published, rel);
  }
  // A path under data/h/ that names no file gets a 404 page of its own, not the app shell.
  assert.match(readFileSync(join(root, 'public/data/h/404.html'), 'utf8'), /Not found/);
  assert.equal(result.files.includes('h/404.html'), false);
  // The listed files are the plain ones; the addressed copies are not part of the pack.
  assert.equal(result.files.some((rel) => rel.startsWith('h/')), false);
});

test('an explicit external data root is used and never falls back to repository data', (t) => {
  const { root, options } = fixture(t);
  const previous = process.env.POBR_DATA_ROOT;
  t.after(() => {
    if (previous === undefined) delete process.env.POBR_DATA_ROOT;
    else process.env.POBR_DATA_ROOT = previous;
  });
  process.env.POBR_DATA_ROOT = options.dataRoot;
  const result = syncData({ destRoot: options.destRoot });
  assert.equal(result.version, '9.1');
  process.env.POBR_DATA_ROOT = join(root, 'missing');
  assert.throws(() => syncData({ destRoot: options.destRoot }), /ENOENT/);
});

test('sync rejects domain and language declarations missing from the inventory', (t) => {
  const { root, write, manifest, options } = fixture(t);
  manifest.domains.overlay = ['gem_quality_stats'];
  write('source/9.1/manifest.json', manifest);
  assert.throws(() => syncData(options), /Declared domain lacks an inventory entry/);
  delete manifest.domains.overlay;
  manifest.languages = ['zh-TW'];
  write('source/9.1/manifest.json', manifest);
  assert.throws(() => syncData(options), /Declared language lacks files/);
  assert.equal(existsSync(join(root, 'public/data/previous.json')), true);
});

test('sync rejects schema 3 envelopes that native loaders cannot deserialize', (t) => {
  const { write, manifest, options } = fixture(t);
  for (const changes of [{ languages: null }, { domains: {} }, { domains: { base: ['skill_gems'], overlay: [1] } }]) {
    write('source/9.1/manifest.json', { ...manifest, ...changes });
    assert.throws(() => syncData(options), /Invalid runtime snapshot manifest/);
  }
});

test('fingerprint failure leaves the entire published browser directory intact', (t) => {
  const { root, write, options } = fixture(t);
  write('source/9.1/base/skill_gems.json', '[]');
  assert.throws(() => syncData(options), /fingerprint mismatch/);
  assert.equal(readFileSync(join(root, 'public/data/previous.json'), 'utf8'), '{"keep":true}');
  assert.equal(existsSync(join(root, 'public/data/9.1')), false);
});

test('unsafe manifest paths are rejected before touching browser data', (t) => {
  const { root, write, manifest, options } = fixture(t);
  manifest.files = { '../outside.json': '0'.repeat(64) };
  write('source/9.1/manifest.json', manifest);
  assert.throws(() => syncData(options), /Invalid snapshot path/);
  assert.equal(existsSync(join(root, 'public/data/previous.json')), true);
});

test('legacy data still syncs while excluding maintenance reports', (t) => {
  const { root, write, options } = fixture(t);
  write('source/9.1/manifest.json', { schema_version: 2, poe_version: '9.1' });
  const result = syncData(options);
  assert.equal(result.files.includes('generated/parsed_mods.json'), false);
  assert.equal(existsSync(join(root, 'public/data/9.1/base/skill_gems.json')), true);
});

test('failed publication and rollback retain the only original backup', (t) => {
  const { root, options } = fixture(t);
  let calls = 0;
  assert.throws(() => syncData(options, (source, target) => {
    calls += 1;
    if (calls > 1) throw new Error('injected rename failure');
    renameSync(source, target);
  }), /original backup retained/);
  const staged = readdirSync(join(root, 'public')).find((name) => name.startsWith('.data-sync-'));
  assert.ok(staged);
  assert.equal(readFileSync(join(root, 'public', staged, 'previous/previous.json'), 'utf8'), '{"keep":true}');
});

test('runtime release sync publishes exactly the verified blocks with no legacy copies or evidence', (t) => {
  const { root, options } = fixture(t);
  runtimePackFixture(join(root, 'release'));
  const packRoot = join(root, 'release/player');
  const manifest = JSON.parse(readFileSync(join(packRoot, 'manifest.json')));
  const result = syncData({ ...options, packRoot, expectedId: manifest.release_id });
  assert.deepEqual(result, manifest);
  assert.deepEqual(JSON.parse(readFileSync(join(options.destRoot, 'manifest.json'))), result);
  for (const spec of manifest.blocks) {
    assert.deepEqual(readFileSync(join(options.destRoot, 'h', spec.sha256.slice(0, 16), spec.path)), readFileSync(join(packRoot, spec.path)));
  }
  assert.deepEqual(readdirSync(options.destRoot).sort(), ['h', 'manifest.json']);
  assert.equal(existsSync(join(options.destRoot, 'build-receipt.json')), false);
  assert.equal(existsSync(join(options.destRoot, '9.1')), false);
  assert.throws(() => syncData({ ...options, packRoot, expectedId: '0'.repeat(64) }), /Command failed/);
  assert.deepEqual(JSON.parse(readFileSync(join(options.destRoot, 'manifest.json'))), result);
  writeFileSync(join(packRoot, 'blocks/rules.json'), '{}');
  assert.throws(() => syncData({ ...options, packRoot }), /Command failed/);
  assert.deepEqual(JSON.parse(readFileSync(join(options.destRoot, 'manifest.json'))), result);
});

test('runtime release publication failure restores the previous directory', (t) => {
  const { root, options } = fixture(t);
  runtimePackFixture(join(root, 'release'));
  let calls = 0;
  assert.throws(() => syncData({ ...options, packRoot: join(root, 'release/player') }, (source, target) => {
    calls += 1;
    if (calls === 2) throw new Error('injected release publication failure');
    renameSync(source, target);
  }), /injected release publication failure/);
  assert.equal(calls, 3);
  assert.deepEqual(readdirSync(options.destRoot), ['previous.json']);
  assert.equal(readFileSync(join(options.destRoot, 'previous.json'), 'utf8'), '{"keep":true}');
  assert.equal(readdirSync(dirname(options.destRoot)).some((name) => name.startsWith('.data-sync-')), false);
});

test('runtime release rollback failure preserves the original backup for recovery', (t) => {
  const { root, options } = fixture(t);
  runtimePackFixture(join(root, 'release'));
  let calls = 0;
  assert.throws(() => syncData({ ...options, packRoot: join(root, 'release/player') }, (source, target) => {
    calls += 1;
    if (calls > 1) throw new Error('injected release rename failure');
    renameSync(source, target);
  }), /original backup retained/);
  assert.equal(calls, 3);
  assert.equal(existsSync(options.destRoot), false);
  const staged = readdirSync(dirname(options.destRoot)).filter((name) => name.startsWith('.data-sync-'));
  assert.equal(staged.length, 1);
  const backup = join(dirname(options.destRoot), staged[0], 'previous');
  assert.deepEqual(readdirSync(backup), ['previous.json']);
  assert.equal(readFileSync(join(backup, 'previous.json'), 'utf8'), '{"keep":true}');
  renameSync(backup, options.destRoot);
  assert.equal(readFileSync(join(options.destRoot, 'previous.json'), 'utf8'), '{"keep":true}');
});
