import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createDisplayTexts, projectDisplayTexts } from './native-display-texts.mjs';

const row = (id, english, localized, extra = {}) => ({
  table: 'BaseItemTypes', field: 'Name', id, english, localized,
  templates: { english: { syntax_valid: true, placeholders: [], hash_slots: 0 } }, ...extra,
});
const project = records => projectDisplayTexts({ game_patch: 'test', realm: 'international', locale: 'zh-TW', records }, query => {
  const match = records.find(r => r.table === query.table && r.field === query.field && r.id === query.id);
  assert.equal(query.english, match.english ?? '');
  return { status: match.localized === null ? 'fallback' : 'translated', text: match.localized ?? query.english,
    reason: match.localized === null ? 'unchanged-text' : null, ids: [match.id] };
}, 'a'.repeat(64));

test('preserves scoped identity while blocking conflicting and incomplete English homonyms', () => {
  const { sidecar, conflicts } = project([
    row('a', 'Same', '甲'), row('b', 'Same', '乙'),
    row('c', 'Other', '丙'), row('d', 'Other', null), row('e', 'Unique', '丁'),
  ]);
  assert.deepEqual(sidecar.entries, { Other: null, Same: null, Unique: '丁' });
  assert.deepEqual(sidecar.scopes['BaseItemTypes.Name'].a, ['Same', '甲']);
  assert.deepEqual(sidecar.scopes['BaseItemTypes.Name'].d, ['Other', null]);
  assert.equal(conflicts.length, 2);
});

test('markup aliases and different tables participate in the same unscoped ambiguity check', () => {
  const { sidecar } = project([
    row('a', '[Key|Same]', '[Key|甲]'), row('b', 'Same', '乙', { table: 'PassiveSkills' }),
    row('c', '[Unchanged]', null), row('d', 'Agrees', '同'), row('e', 'Agrees', '同'),
  ]);
  assert.equal(sidecar.entries['[Key|Same]'], '[Key|甲]');
  assert.equal(sidecar.entries.Same, null);
  assert.equal(sidecar.entries.Unchanged, null);
  assert.equal(sidecar.entries.Agrees, '同');
});

test('retains unrendered placeholder, empty and unknown syntax records only in their source scope', () => {
  const { sidecar, decisions } = project([
    row('a', 'Gain {0}', '獲得 {0}', { templates: { english: { syntax_valid: true, placeholders: [{}], hash_slots: 0 } } }),
    row('b', 'Broken [', null, { templates: { english: { syntax_valid: false, placeholders: [], hash_slots: 0 } } }),
    row('c', '', null), row('d', 'Gain #', null, { templates: { english: { syntax_valid: true, placeholders: [], hash_slots: 1 } } }),
  ]);
  assert.deepEqual(sidecar.entries, {});
  assert.equal(Object.keys(sidecar.scopes['BaseItemTypes.Name']).length, 4);
  assert.equal(decisions.length, 4);
});

test('Words retain numeric decision identities and prototype-like literals without an unused browser scope', () => {
  const { sidecar, decisions } = project([row(123, '__proto__', '詞', { table: 'Words', field: 'Text2' })]);
  assert.equal(Object.getPrototypeOf(sidecar.entries), Object.prototype);
  assert.equal(Object.hasOwn(sidecar.entries, '__proto__'), true);
  assert.equal(sidecar.entries.__proto__, '詞');
  assert.equal(sidecar.scopes['Words.Text2'], undefined);
  assert.deepEqual(decisions[0].identity, ['Words', 'Text2', 123]);
});

test('duplicate source identities are rejected instead of last-write winning', () => {
  assert.throws(() => project([row('a', 'One', '一'), row('a', 'One', '一')]), /Duplicate/);
});

test('declared scopes change exact indexes without dropping ambiguity participants', () => {
  const records = [row('a', 'Same', '甲'), row('b', 'Same', null, { table: 'PassiveSkills' })];
  const { sidecar } = projectDisplayTexts({ records }, query => {
    const match = records.find(r => r.table === query.table);
    return { status: match.localized ? 'translated' : 'fallback', text: match.localized ?? match.english };
  }, 'a'.repeat(64), { scopes: ['PassiveSkills.Name'], literals: 'all-catalog-conflict-participants' });
  assert.deepEqual(Object.keys(sidecar.scopes), ['PassiveSkills.Name']);
  assert.equal(sidecar.entries.Same, null);
  assert.throws(() => projectDisplayTexts({ records }, () => ({}), 'a'.repeat(64), {
    scopes: [], literals: 'translated-only',
  }), /Invalid runtime projection/);
});

test('duplicate target English candidates block other stable IDs and stripped aliases without choosing a row', () => {
  const rejected = row('duplicate', null, null, {
    templates: { english: null },
    candidates: { english: [{ text: '[Key|Same]' }, { text: 'Other' }, { text: '[Key|Same]' }, { text: '' }] },
  });
  for (const records of [
    [rejected, row('a', '[Key|Same]', '甲'), row('b', 'Same', '乙'), row('c', 'Other', '丙')],
    [row('a', '[Key|Same]', '甲'), row('b', 'Same', '乙'), row('c', 'Other', '丙'), rejected],
  ]) {
    const { sidecar } = project(records);
    assert.deepEqual(sidecar.entries, { '[Key|Same]': null, Same: null, Other: null });
    assert.deepEqual(sidecar.scopes['BaseItemTypes.Name'].duplicate, [null, null]);
    assert.deepEqual(sidecar.scopes['BaseItemTypes.Name'].a, ['[Key|Same]', '甲']);
  }
});

test('additional English candidates block stale text without blocking the exact accepted query', () => {
  const { sidecar } = project([
    row('a', 'Current', '目前', { candidates: { english: [
      { text: 'Current' }, { text: '[Old|Former]' }, { text: '   ' },
    ] } }),
    row('b', 'Former', '旧'),
  ]);
  assert.deepEqual(sidecar.entries, { Current: '目前', '[Old|Former]': null, Former: null });
});

test('browser indexes retain only active exact-ID scopes while all fields still block ambiguous literals', () => {
  const records = [
    row('base', 'Same', '甲'),
    row('class', 'Warrior', '战士', { table: 'Characters' }),
    row('asc', 'Titan', '泰坦', { table: 'Ascendancy' }),
    row('skill', 'Same', null, { table: 'ActiveSkills', field: 'Description' }),
    row('empty', '', null, { table: 'Words', field: 'Inflection' }),
  ];
  const { sidecar, decisions } = project(records);
  assert.deepEqual(Object.keys(sidecar.scopes), ['Ascendancy.Name', 'BaseItemTypes.Name', 'Characters.Name']);
  assert.deepEqual(sidecar.entries, { Same: null, Titan: '泰坦', Warrior: '战士' });
  assert.equal(decisions.length, records.length);
  assert.throws(() => project([...records, records[4]]), /Duplicate/);
});

function fixture(t, version, modules, options = {}) {
  const cache = fileURLToPath(new URL('../../.cache/', import.meta.url));
  fs.mkdirSync(cache, { recursive: true });
  const root = fs.mkdtempSync(path.join(cache, 'display-contract-test-'));
  t.after(() => fs.rmSync(root, { recursive: true }));
  const pin = bytes => ({ sha256: createHash('sha256').update(bytes).digest('hex'), bytes: Buffer.byteLength(bytes) });
  const write = (file, value) => { fs.writeFileSync(path.join(root, file), value); return pin(value); };
  const identity = { game_patch: 'test', realm: 'international', locale: 'zh-CN' };
  const catalog = { schema: `poe2-native-catalog/v${version}`, ...identity, records: options.records ?? [] };
  const verification = '{"original":"验证"}\n';
  if (version === 3) catalog.fk_verification = { file: 'verification.json', ...write('verification.json', verification) };
  const catalogPin = write('catalog.json', JSON.stringify(catalog));
  const manifest = { schema: `poe2-native-catalog-manifest/v${version}`, game: 'poe2', status: 'candidate',
    production_ready: false, ...identity, files: {
      'catalog.json': catalogPin, 'request.json': write('request.json', '{}'),
    } };
  if (version === 3) manifest.files['verification.json'] = pin(verification);
  fs.mkdirSync(path.join(root, 'src'));
  const library = { root, files: {} };
  for (const module of ['native-catalog', 'native-names', 'strict-json',
    ...(modules >= 5 ? ['native-catalog-binding', 'native-localization'] : []),
    ...(modules === 6 ? ['native-catalog-fk-binding'] : [])]) {
    const source = module === 'native-catalog' ? `export const createCatalogLookup = (catalog, expected) => {
      if (catalog.schema.endsWith('/v3') && (!Buffer.isBuffer(expected.verification)
        || expected.verification.toString() !== ${JSON.stringify(verification)})) throw new Error('Original sidecar required');
      return query => ({ status: 'fallback', text: query.english, reason: 'missing-identity-or-text', ids: [] });
    };`
      : module === 'strict-json' ? 'export const strictJson = JSON.parse;' : 'export {};';
    library.files[`src/${module}.mjs`] = write(`src/${module}.mjs`, source).sha256;
  }
  manifest.producer = { ...library.files };
  const request = { schema: 'pobr-native-display-texts-request/v1', ...identity, library,
    catalog: { manifest: path.join(root, 'manifest.json'), ...write('manifest.json', JSON.stringify(manifest)) } };
  const output = { root, manifest, catalog, library, request, write, pin, out: path.join(root, 'out') };
  if (options.supplement) addSupplement(output, pin, write);
  return output;
}

// A closed stub of the library's reviewed-supplement module: it accepts only the
// original verification Buffer, the catalog pin and the externally pinned runtime,
// rejects any non-reviewed record, and translates only an exact full context.
function addSupplement(input, pin, write) {
  const records = [{ source: 0, entry_id: 'e'.repeat(64), table: 'BaseItemTypes', field: 'Name', id: 'flask',
    english: 'Flask', context: { ItemClass: 'Flask' }, text: '药剂', origin: 'agent', author: 'fixture-agent',
    note: 'synthetic', status: 'reviewed', review: { by: 'fixture-reviewer', note: 'synthetic' } }];
  const runtime = { schema: 'poe2-reviewed-native-supplements/v1', status: 'candidate', production_ready: false,
    sources: [{ game: 'poe2', game_patch: 'test', realm: 'international', locale: 'zh-CN',
      catalog_sha256: input.manifest.files['catalog.json'].sha256,
      verification: { bytes: input.catalog.fk_verification.bytes, sha256: input.catalog.fk_verification.sha256 } }],
    records };
  const directory = path.join(input.root, 'supplement');
  fs.mkdirSync(directory);
  const runtimePin = pin(JSON.stringify(runtime) + '\n');
  fs.writeFileSync(path.join(directory, 'supplements.json'), JSON.stringify(runtime) + '\n');
  fs.writeFileSync(path.join(directory, 'request.json'), '{}');
  const module = `import { createHash } from 'node:crypto';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const records = ${JSON.stringify(records)};
const source = ${JSON.stringify(runtime.sources[0])};
const catalogSha = ${JSON.stringify(runtime.sources[0].catalog_sha256)};
export const createReviewedSupplementLookup = (inputs, runtime) => {
  if (inputs.length !== 1 || !Buffer.isBuffer(inputs[0].verification)) throw new Error('Original sidecar buffer required');
  if (inputs[0].sha256 !== catalogSha || digest(inputs[0].bytes) !== catalogSha) throw new Error('Catalog pin differs');
  if (digest(runtime.bytes) !== runtime.sha256) throw new Error('Runtime pin differs');
  const parsed = JSON.parse(runtime.bytes.toString('utf8'));
  if (parsed.sources.length !== 1 || JSON.stringify(parsed.sources[0]) !== JSON.stringify(source)) throw new Error('Runtime source differs');
  if (parsed.records.some(row => row.status !== 'reviewed')) throw new Error('Unreviewed supplement record');
  globalThis.supplementQueries = [];
  return query => {
    globalThis.supplementQueries.push(query);
    const row = records.find(r => r.table === query.table && r.field === query.field && r.id === query.id
      && r.english === query.english && JSON.stringify(r.context) === JSON.stringify(query.context));
    return row ? { status: 'translated', text: row.text, reason: 'reviewed-supplement', ids: [row.id], origin: row.origin }
      : { status: 'fallback', text: query.english, reason: 'missing-identity-or-text', ids: [], origin: null };
  };
};
`;
  const modulePin = write('src/translation-workbench.mjs', module);
  const producer = { ...Object.fromEntries(Object.entries(input.library.files)
    .map(([file, sha]) => [path.basename(file), sha])), 'translation-workbench.mjs': modulePin.sha256 };
  const supplementManifest = { schema: 'poe2-reviewed-native-supplements-manifest/v1', status: 'candidate',
    production_ready: false, files: { 'supplements.json': runtimePin,
      'request.json': pin(fs.readFileSync(path.join(directory, 'request.json'))) }, producer };
  const manifestPin = write('supplement/manifest.json', JSON.stringify(supplementManifest));
  // The catalog manifest producer stays the official closure; the extra module is
  // pinned only by this separate supplement block, so the official guard is unchanged.
  input.request.supplement = { manifest: path.join(directory, 'manifest.json'), ...manifestPin,
    module: { file: 'src/translation-workbench.mjs', revision: 'f'.repeat(40), ...modulePin } };
  input.supplementManifest = supplementManifest;
  input.runtimePin = runtimePin;
}

test('pinned catalog versions admit their complete library closures without changing old v1 requests', async t => {
  for (const [version, modules] of [[1, 3], [1, 5], [2, 5], [1, 6], [2, 6], [3, 6]]) {
    const input = fixture(t, version, modules);
    const receipt = await createDisplayTexts(input.request, input.out);
    assert.equal(receipt.decisions.length, 0);
    assert.equal(JSON.parse(fs.readFileSync(path.join(input.out, 'display_texts.json'))).locale, 'zh-CN');
  }
});

test('v3 requires the original pinned verification buffer and complete six-module closure', async t => {
  const missing = fixture(t, 3, 6);
  fs.unlinkSync(path.join(missing.root, 'verification.json'));
  await assert.rejects(createDisplayTexts(missing.request, missing.out), /ENOENT/);
  const corrupt = fixture(t, 3, 6);
  corrupt.write('verification.json', 'corrupt');
  await assert.rejects(createDisplayTexts(corrupt.request, corrupt.out), /size differs|SHA256 mismatch/);
  const mismatched = fixture(t, 3, 6);
  mismatched.catalog.fk_verification.sha256 = '0'.repeat(64);
  mismatched.manifest.files['catalog.json'] = mismatched.write('catalog.json', JSON.stringify(mismatched.catalog));
  Object.assign(mismatched.request.catalog, mismatched.write('manifest.json', JSON.stringify(mismatched.manifest)));
  await assert.rejects(createDisplayTexts(mismatched.request, mismatched.out), /verification pin differs/);
  const incomplete = fixture(t, 3, 5);
  await assert.rejects(createDisplayTexts(incomplete.request, incomplete.out), /Invalid library closure/);
  const substituted = fixture(t, 3, 6);
  substituted.manifest.producer['src/native-catalog.mjs'] = '0'.repeat(64);
  Object.assign(substituted.request.catalog, substituted.write('manifest.json', JSON.stringify(substituted.manifest)));
  await assert.rejects(createDisplayTexts(substituted.request, substituted.out), /producer closure mismatch/);
  for (const input of [missing, corrupt, mismatched, incomplete, substituted]) assert.equal(fs.existsSync(input.out), false);
});

test('v2 rejects incomplete or unpinned import closures before publishing', async t => {
  const old = fixture(t, 2, 3);
  await assert.rejects(createDisplayTexts(old.request, old.out), /Invalid library closure/);
  assert.equal(fs.existsSync(old.out), false);
  const changed = fixture(t, 2, 5);
  changed.write('src/native-catalog-binding.mjs', 'throw new Error("must never execute");');
  await assert.rejects(createDisplayTexts(changed.request, changed.out), /SHA256 mismatch/);
  assert.equal(fs.existsSync(changed.out), false);
});

test('old v1/v2 catalogs keep independently pinned compatible lookup implementations', async t => {
  for (const version of [1, 2]) {
    const input = fixture(t, version, 5);
    input.manifest.producer['src/native-catalog.mjs'] = '0'.repeat(64);
    Object.assign(input.request.catalog, input.write('manifest.json', JSON.stringify(input.manifest)));
    await createDisplayTexts(input.request, input.out);
  }
});

test('a pinned catalog still rejects a mismatched manifest version', async t => {
  const input = fixture(t, 2, 5);
  input.catalog.schema = 'poe2-native-catalog/v1';
  input.manifest.files['catalog.json'] = input.write('catalog.json', JSON.stringify(input.catalog));
  Object.assign(input.request.catalog, input.write('manifest.json', JSON.stringify(input.manifest)));
  await assert.rejects(createDisplayTexts(input.request, input.out), /schema differs/);
  assert.equal(fs.existsSync(input.out), false);
});

test('Git-delivered JSON has no fixed size cap and still requires exact bytes and SHA', async t => {
  const input = fixture(t, 2, 5);
  // Cross the former manifest cap without a large allocation in every test run.
  Object.assign(input.request.catalog, input.write('manifest.json', JSON.stringify(input.manifest) + ' '.repeat(1024 * 1024)));
  await createDisplayTexts(input.request, input.out);
  input.manifest.files['catalog.json'].bytes += 1;
  Object.assign(input.request.catalog, input.write('manifest.json', JSON.stringify(input.manifest)));
  await assert.rejects(createDisplayTexts(input.request, input.out + '-size'), /size differs/);
  input.manifest.files['catalog.json'].bytes -= 1;
  input.manifest.files['catalog.json'].sha256 = '0'.repeat(64);
  Object.assign(input.request.catalog, input.write('manifest.json', JSON.stringify(input.manifest)));
  await assert.rejects(createDisplayTexts(input.request, input.out + '-hash'), /SHA256 mismatch/);
  assert.equal(fs.existsSync(input.out + '-size'), false);
  assert.equal(fs.existsSync(input.out + '-hash'), false);
});

const supplementRecords = [
  { table: 'BaseItemTypes', field: 'Name', id: 'flask', english: 'Flask', localized: null,
    context: { english: { ItemClass: 'Flask' }, localized: null },
    templates: { english: { syntax_valid: true, placeholders: [], hash_slots: 0 }, localized: null } },
  { table: 'BaseItemTypes', field: 'Name', id: 'sword', english: 'Sword', localized: null,
    context: { english: { ItemClass: 'One Hand Swords' }, localized: null },
    templates: { english: { syntax_valid: true, placeholders: [], hash_slots: 0 }, localized: null } },
];

test('a pinned reviewed supplement projects exact-context text and keeps the official fallback', async t => {
  const input = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  const receipt = await createDisplayTexts(input.request, input.out);
  const sidecar = JSON.parse(fs.readFileSync(path.join(input.out, 'display_texts.json')));
  assert.deepEqual(sidecar.entries, { Flask: '药剂', Sword: null });
  assert.deepEqual(sidecar.scopes['BaseItemTypes.Name'],
    { flask: ['Flask', '药剂'], sword: ['Sword', null] });
  assert.equal(sidecar.catalog_sha256, input.manifest.files['catalog.json'].sha256);
  // Only the complete English context of each row reaches the library lookup.
  const scope = { game_patch: 'test', realm: 'international', locale: 'zh-CN' };
  assert.deepEqual(globalThis.supplementQueries, [
    { ...scope, table: 'BaseItemTypes', field: 'Name', id: 'flask', english: 'Flask',
      context: { ItemClass: 'Flask' } },
    { ...scope, table: 'BaseItemTypes', field: 'Name', id: 'sword', english: 'Sword',
      context: { ItemClass: 'One Hand Swords' } },
  ]);
  assert.equal(receipt.supplement.records, 1);
  assert.equal(receipt.supplement.catalog_sha256, input.manifest.files['catalog.json'].sha256);
  assert.deepEqual(receipt.supplement.runtime,
    { bytes: input.runtimePin.bytes, sha256: input.runtimePin.sha256 });
  assert.deepEqual(receipt.supplement.module, input.request.supplement.module);
  assert.deepEqual(receipt.supplement.sources, [{
    game: 'poe2', game_patch: 'test', realm: 'international', locale: 'zh-CN',
    catalog_sha256: input.manifest.files['catalog.json'].sha256,
    verification: { bytes: input.catalog.fk_verification.bytes, sha256: input.catalog.fk_verification.sha256 },
  }]);
  for (const file of ['supplement/manifest.json', 'supplement/supplements.json', 'supplement/request.json',
    'src/translation-workbench.mjs']) {
    assert.equal(typeof receipt.inputs[path.join(input.root, file)], 'object', file);
  }
  assert.deepEqual(fs.readdirSync(input.out).sort(), ['display_texts.json', 'receipt.json']);
});

test('reviewed supplement pins, module closure and catalog version fail closed', async t => {
  const cases = [];
  const wrongModule = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  wrongModule.request.supplement.module.sha256 = '0'.repeat(64);
  cases.push([wrongModule, /SHA256 mismatch/]);
  const missingModule = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  fs.unlinkSync(path.join(missingModule.root, 'src/translation-workbench.mjs'));
  cases.push([missingModule, /ENOENT/]);
  const movedRevision = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  movedRevision.request.supplement.module.revision = 'latest';
  cases.push([movedRevision, /Invalid supplement module pin/]);
  const missingModuleSize = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  delete missingModuleSize.request.supplement.module.bytes;
  cases.push([missingModuleSize, /Invalid supplement module pin/]);
  const missingManifestSize = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  delete missingManifestSize.request.supplement.bytes;
  cases.push([missingManifestSize, /Invalid supplement manifest pin/]);
  const wrongManifest = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  wrongManifest.request.supplement.sha256 = '0'.repeat(64);
  cases.push([wrongManifest, /SHA256 mismatch/]);
  const wrongRuntime = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  fs.writeFileSync(path.join(wrongRuntime.root, 'supplement/supplements.json'),
    JSON.stringify({ ...JSON.parse(fs.readFileSync(path.join(wrongRuntime.root, 'supplement/supplements.json'))),
      records: [] }) + '\n');
  cases.push([wrongRuntime, /size differs|SHA256 mismatch/]);
  const wrongProducer = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  wrongProducer.supplementManifest.producer['native-catalog.mjs'] = '0'.repeat(64);
  Object.assign(wrongProducer.request.supplement,
    wrongProducer.write('supplement/manifest.json', JSON.stringify(wrongProducer.supplementManifest)));
  cases.push([wrongProducer, /Supplement producer closure mismatch/]);
  const extraProducer = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  extraProducer.supplementManifest.producer['extra.mjs'] = '0'.repeat(64);
  Object.assign(extraProducer.request.supplement,
    extraProducer.write('supplement/manifest.json', JSON.stringify(extraProducer.supplementManifest)));
  cases.push([extraProducer, /Supplement producer closure mismatch/]);
  const oldCatalog = fixture(t, 2, 5);
  oldCatalog.request.supplement = { module: { file: 'src/translation-workbench.mjs', revision: 'f'.repeat(40) } };
  cases.push([oldCatalog, /Reviewed supplements require a v3 catalog/]);
  for (const [input, error] of cases) {
    await assert.rejects(createDisplayTexts(input.request, input.out), error);
    assert.equal(fs.existsSync(input.out), false);
  }
});

test('a library that refuses unreviewed or foreign records publishes nothing', async t => {
  const draft = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  const file = path.join(draft.root, 'supplement/supplements.json');
  const runtime = JSON.parse(fs.readFileSync(file));
  runtime.records[0].status = 'draft';
  const raw = JSON.stringify(runtime) + '\n';
  fs.writeFileSync(file, raw);
  draft.supplementManifest.files['supplements.json'] = draft.pin(raw);
  Object.assign(draft.request.supplement, draft.write('supplement/manifest.json',
    JSON.stringify(draft.supplementManifest)));
  await assert.rejects(createDisplayTexts(draft.request, draft.out), /Unreviewed supplement record/);
  assert.equal(fs.existsSync(draft.out), false);
  const foreign = fixture(t, 3, 6, { supplement: true, records: supplementRecords });
  const foreignFile = path.join(foreign.root, 'supplement/supplements.json');
  const foreignRuntime = JSON.parse(fs.readFileSync(foreignFile));
  foreignRuntime.sources[0].realm = 'china';
  const foreignRaw = JSON.stringify(foreignRuntime) + '\n';
  fs.writeFileSync(foreignFile, foreignRaw);
  foreign.supplementManifest.files['supplements.json'] = foreign.pin(foreignRaw);
  Object.assign(foreign.request.supplement, foreign.write('supplement/manifest.json',
    JSON.stringify(foreign.supplementManifest)));
  await assert.rejects(createDisplayTexts(foreign.request, foreign.out), /Runtime source differs/);
  assert.equal(fs.existsSync(foreign.out), false);
});

test('official-only requests still publish the same sidecar without a supplement block', async t => {
  delete globalThis.supplementQueries;
  const input = fixture(t, 3, 6, { records: supplementRecords });
  const receipt = await createDisplayTexts(input.request, input.out);
  assert.equal(Object.hasOwn(receipt, 'supplement'), false);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(input.out, 'display_texts.json'))).entries,
    { Flask: null, Sword: null });
  assert.deepEqual(globalThis.supplementQueries, undefined);
});
