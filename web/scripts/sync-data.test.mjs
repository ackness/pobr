import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { syncData } from './sync-data.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'pobr-sync-data-'));
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
