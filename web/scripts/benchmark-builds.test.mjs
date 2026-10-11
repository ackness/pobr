import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { deflateSync } from 'node:zlib';
import { discoverBuilds } from './benchmark-builds.mjs';

const hash = value => createHash('sha256').update(value).digest('hex');
const xml = '<PathOfBuilding2><Build level="90"/></PathOfBuilding2>';
const encode = value => deflateSync(value).toString('base64url');

async function workspace(t) {
  const path = await mkdtemp(join(tmpdir(), 'benchmark-builds-'));
  t.after(() => rm(path, { recursive: true, force: true }));
  return path;
}

async function fixture(root, name, changes = {}) {
  const directory = join(root, name);
  await mkdir(directory, { recursive: true });
  const code = changes.code ?? encode(xml);
  const meta = {
    name,
    source: { site: 'poe.ninja', url: 'https://poe.ninja/poe2/builds/season/character/account/NaN?label=Infinity',
      league: 'A season', fetched_at: '2026-10-11', account: 'account', character: 'NaN' },
    character: { class: 'Witch', ascendancy: 'Blood Mage', level: 90, main_skill: 'Infinity' },
    pob: { code_sha256: hash(code) },
    ...changes.meta,
  };
  const metadata = changes.rawMeta ?? JSON.stringify(meta);
  await writeFile(join(directory, 'code.txt'), `${code}\n`);
  await writeFile(join(directory, 'meta.json'), metadata);
  return { directory, code, meta, metadata };
}

test('discovery sorts all roots, preserves original source and character, and never mutates files or paths', async t => {
  const root = await workspace(t);
  const first = join(root, 'first');
  const second = join(root, 'second');
  const z = await fixture(first, 'zeta');
  const a = await fixture(second, 'alpha');
  const paths = [second, first];
  const before = [...paths];
  const result = await discoverBuilds(paths);
  assert.deepEqual(paths, before);
  assert.deepEqual(result.map(build => build.fixture), ['alpha', 'zeta']);
  for (const [build, input] of [[result[0], a], [result[1], z]]) {
    assert.equal(build.code, input.code);
    assert.equal(build.xml, xml);
    assert.equal(build.sha256, hash(input.code));
    assert.equal(build.xmlSha256, hash(xml));
    assert.deepEqual(build.source, input.meta.source);
    assert.deepEqual(build.character, input.meta.character);
    assert.equal(await readFile(join(input.directory, 'meta.json'), 'utf8'), input.metadata);
    assert.equal(await readFile(join(input.directory, 'code.txt'), 'utf8'), `${input.code}\n`);
  }
  assert.deepEqual(await discoverBuilds([...paths].reverse()), result);
});

test('historical nonfinite numeric tokens normalize without touching quoted source strings', async t => {
  const root = await workspace(t);
  const input = await fixture(root, 'legacy');
  const raw = JSON.stringify(input.meta).replace(/}$/, ',"stats":{"a":NaN,"b":Infinity,"c":-Infinity}}');
  await writeFile(join(input.directory, 'meta.json'), raw);
  const [build] = await discoverBuilds([root]);
  assert.deepEqual(build.source, input.meta.source);
  assert.equal(build.character.main_skill, 'Infinity');
  assert.equal(await readFile(join(input.directory, 'meta.json'), 'utf8'), raw);
});

test('unknown legacy provenance stays unknown without excluding a valid hashed build', async t => {
  const root = await workspace(t);
  const source = { site: 'poe.ninja', url: null, league: null, fetched_at: null, game_version: null };
  await fixture(root, 'legacy-unknown-source', { meta: { source } });
  const [build] = await discoverBuilds([root]);
  assert.deepEqual(build.source, source);
  assert.equal(build.fixture, 'legacy-unknown-source');
});

test('missing roots and empty roots are actionable errors rather than empty successful corpora', async t => {
  const root = await workspace(t);
  await assert.rejects(discoverBuilds([join(root, 'missing')]), /Cannot read required build root/);
  await assert.rejects(discoverBuilds([root]), /No fixture directories/);
  for (const paths of [[], null, ['']]) await assert.rejects(discoverBuilds(paths), /Build directories/);
});

test('discovery aggregates missing metadata and missing code without publishing a successful subset', async t => {
  const root = await workspace(t);
  await fixture(root, 'good');
  const a = await fixture(root, 'missing-meta');
  const b = await fixture(root, 'missing-code');
  await rm(join(a.directory, 'meta.json'));
  await rm(join(b.directory, 'code.txt'));
  await assert.rejects(discoverBuilds([root]), error => {
    assert.match(error.message, /missing-meta.*meta\.json/);
    assert.match(error.message, /missing-code.*code\.txt/);
    assert.match(error.message, /failed \(2\)/);
    return true;
  });
});

test('malformed metadata and missing or incorrect hashes fail before execution', async t => {
  const root = await workspace(t);
  const cases = [
    { rawMeta: '{', expected: /JSON|property|Unexpected/i },
    { rawMeta: 'null', expected: /Malformed fixture metadata/ },
    { meta: { source: {} }, expected: /source metadata/ },
    { meta: { character: { class: 'Witch' } }, expected: /character metadata/ },
    { meta: { pob: {} }, expected: /code_sha256/ },
    { meta: { pob: { code_sha256: 'bad' } }, expected: /code_sha256/ },
    { meta: { pob: { code_sha256: '0'.repeat(64) } }, expected: /SHA-256 mismatch/ },
  ];
  for (const [index, entry] of cases.entries()) {
    const directory = join(root, String(index));
    await fixture(directory, 'invalid', entry);
    await assert.rejects(discoverBuilds([directory]), entry.expected);
  }
});

test('empty codes, invalid base64url, corrupted zlib, invalid UTF-8 and non-PoB2 XML are rejected', async t => {
  const root = await workspace(t);
  const inputs = ['', 'not a code!', Buffer.from('not zlib').toString('base64url'),
    encode(Buffer.from([0xff, 0xfe])), encode('<PathOfBuilding><Build/></PathOfBuilding>'),
    encode('<PathOfBuilding2><Build/>'), encode('<PathOfBuilding2></PathOfBuilding2>')];
  for (const [index, code] of inputs.entries()) {
    const directory = join(root, String(index));
    await fixture(directory, 'invalid', { code });
    await assert.rejects(discoverBuilds([directory]));
  }
});

test('duplicate fixture identities across roots fail even when their codes differ', async t => {
  const root = await workspace(t);
  const first = join(root, 'first');
  const second = join(root, 'second');
  await fixture(first, 'duplicate');
  await fixture(second, 'duplicate', { code: encode(xml.replace('90', '91')) });
  await assert.rejects(discoverBuilds([first, second]), /Duplicate fixture identity: duplicate/);
});
