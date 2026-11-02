// Consume a materialized candidate with an already compiled standalone artifact.
// No downloads, builds, bundled game data, or ambient POBR_* data selection.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { constants, linkSync, lstatSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { snapshotFiles } from './sync-data.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const isHash = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
// The materializer's small bundle receipt uses sorted-key, compact UTF-8 JSON.
const canonical = value => JSON.stringify(value, (_, item) => object(item)
  ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : item);
const workedBuilds = ['monk-invoker-frost-bomb', 'mercenary-tactician-wolf-pack', 'ranger-deadeye-explosive-grenade'];
// Deliberately outside game modifier grammar; this must remain a dropped input.
const unsupportedSentinel = 'POBR_SYNTHETIC_RUNTIME_CANDIDATE_UNSUPPORTED_7B21A6CF_DO_NOT_MODEL';
const savedSentinelDiagnostic = `Config: ${unsupportedSentinel}`;

function safeRelative(name) {
  assert.ok(typeof name === 'string' && name.length && !/[\\:\x00-\x1f]/.test(name)
    && name.split('/').every(part => part && part !== '.' && part !== '..'), `Unsafe input path: ${name}`);
  return name;
}

function explicitPath(value) {
  assert.ok(typeof value === 'string' && value.length && !/[\\\x00]/.test(value)
    && !value.split('/').includes('..'), 'Explicit paths must not traverse parent directories');
  return resolve(value);
}

function noSymlinks(path, missingLeaf = false) {
  const parent = dirname(path);
  if (parent !== path) noSymlinks(parent);
  const stat = lstatSync(path, { bigint: true, throwIfNoEntry: !missingLeaf });
  assert.ok(!stat?.isSymbolicLink(), `Symlink input/output path: ${path}`);
  return stat;
}

const stamp = stat => [stat.dev, stat.ino, stat.mode, stat.size, stat.mtimeNs, stat.ctimeNs].join(':');

function inputReader() {
  const pins = new Map();
  function read(path) {
    const before = noSymlinks(path);
    assert.ok(before.isFile(), `Expected a regular input file: ${path}`);
    const bytes = readFileSync(path, { flag: constants.O_RDONLY | constants.O_NOFOLLOW });
    const pin = { bytes: bytes.length, sha256: sha256(bytes), stamp: stamp(before) };
    assert.equal(stamp(noSymlinks(path)), pin.stamp, `Input changed during smoke: ${path}`);
    if (pins.has(path)) assert.deepEqual(pin, pins.get(path), `Input changed during smoke: ${path}`);
    else pins.set(path, pin);
    return bytes;
  }
  return { read, json: path => JSON.parse(read(path).toString('utf8')),
    verify: () => { for (const path of pins.keys()) read(path); } };
}

function inventory(root) {
  assert.ok(noSymlinks(root).isDirectory(), `Expected an input directory: ${root}`);
  const files = [];
  const directories = {};
  function visit(directory) {
    directories[relative(root, directory)] = stamp(noSymlinks(directory));
    for (const name of readdirSync(directory).sort()) {
      safeRelative(name);
      const path = join(directory, name);
      const stat = noSymlinks(path);
      if (stat.isDirectory()) visit(path);
      else {
        assert.ok(stat.isFile(), `Nonregular candidate input: ${path}`);
        files.push(relative(root, path).split(sep).join('/'));
      }
    }
  }
  visit(root);
  return { files: files.sort(), directories };
}

function candidate(root, expectedRuntime, inputs) {
  const listing = inventory(root); // Reject links before snapshotFiles can follow them.
  const bundle = inputs.json(join(root, 'bundle.json'));
  assert.ok(object(bundle) && bundle.schema === 'pobr-data-bundle/v1' && bundle.required_manifest_schema === 3,
    'Expected a materialized schema-3 candidate bundle');
  assert.ok(isHash(bundle.bundle_id) && object(bundle.files), 'Invalid candidate bundle identity/inventory');
  assert.ok(typeof bundle.data_version === 'string' && /^[0-9]+(?:\.[0-9]+)+$/.test(bundle.data_version),
    'Invalid candidate data version');
  const { bundle_id, ...payload } = bundle;
  assert.equal(sha256(canonical(payload)), bundle_id, 'Candidate bundle identity mismatch');
  assert.equal(bundle.runtime_identity, expectedRuntime, 'Candidate runtime identity differs from --expected-runtime');
  const dataRoot = join(root, 'data');
  const current = inputs.read(join(dataRoot, 'CURRENT')).toString('utf8');
  assert.equal(current, `${bundle.data_version}\n`, 'Candidate CURRENT/version mismatch');
  assert.equal(inputs.json(join(dataRoot, bundle.data_version, 'manifest.json')).schema_version, 3,
    'Candidate must use snapshot schema 3');
  // Explicit version prevents POBR_DATA_VERSION from selecting another snapshot.
  const snapshot = snapshotFiles({ dataRoot, version: bundle.data_version });
  const sources = new Map([['data/CURRENT', join(dataRoot, 'CURRENT')]]);
  const staged = [];
  for (const [name, source] of [...snapshot.copies].sort(([a], [b]) => a.localeCompare(b))) {
    safeRelative(name);
    assert.ok(!name.startsWith('patch/'), 'Materialized candidates must not contain local user patches');
    sources.set(relative(root, source).split(sep).join('/'), source);
    const bytes = inputs.read(source);
    JSON.parse(bytes.toString('utf8'));
    staged.push({ path: name, source: relative(root, source).split(sep).join('/'),
      bytes: bytes.length, sha256: sha256(bytes), content: bytes.toString('utf8') });
  }
  assert.deepEqual(Object.keys(bundle.files).sort(), [...sources.keys()].sort(), 'Candidate bundle input set mismatch');
  assert.deepEqual(listing.files, ['bundle.json', ...sources.keys()].sort(), 'Candidate contains missing or unlisted inputs');
  for (const [name, source] of sources) {
    safeRelative(name);
    assert.ok(isHash(bundle.files[name]), `Invalid candidate input digest: ${name}`);
    assert.equal(sha256(inputs.read(source)), bundle.files[name], `Candidate input hash mismatch: ${name}`);
  }
  return { bundle, staged, listing, version: snapshot.version };
}

function standalone(root, temp, inputs, expectedSchema) {
  assert.ok(noSymlinks(root).isDirectory(), 'Expected a standalone artifact directory');
  const sums = new Map();
  for (const line of inputs.read(join(root, 'SHA256SUMS')).toString('utf8').trim().split('\n')) {
    const match = /^([a-f0-9]{64})  (.+)$/.exec(line);
    assert.ok(match, 'Invalid package SHA256SUMS');
    const [, digest, name] = match;
    safeRelative(name);
    assert.equal(basename(name), name, 'Package assets must be direct children');
    assert.ok(!sums.has(name), `Duplicate package asset: ${name}`);
    sums.set(name, digest);
    assert.equal(sha256(inputs.read(join(root, name))), digest, `Package asset hash mismatch: ${name}`);
  }
  assert.ok(sums.has('wasm-size.json'), 'Unsealed package size report');
  const report = inputs.json(join(root, 'wasm-size.json'));
  const archive = report.archive;
  assert.ok(archive && /^pobr-wasm-v[0-9][0-9A-Za-z.+-]*-web\.tar\.gz$/.test(archive.name), 'Unsafe package archive name');
  assert.equal(sums.get(archive.name), archive.sha256, 'Archive identity differs from SHA256SUMS');
  const compressed = inputs.read(join(root, archive.name));
  assert.equal(compressed.length, archive.bytes, 'Archive byte size mismatch');
  const frozenArchive = join(temp, 'artifact.tar.gz');
  writeFileSync(frozenArchive, compressed, { flag: 'wx' });
  const prefix = archive.name.replace(/\.tar\.gz$/, '');
  const tar = args => execFileSync('tar', [...args, frozenArchive], {
    maxBuffer: 64 * 1024 * 1024, timeout: 60_000, env: { ...process.env, LC_ALL: 'C', COPYFILE_DISABLE: '1' },
  });
  const members = tar(['-tzf']).toString('utf8').trim().split('\n');
  const normalized = members.map(name => {
    const clean = safeRelative(name.replace(/\/$/, ''));
    assert.ok(/^[A-Za-z0-9.+_/-]+$/.test(clean) && (clean === prefix || clean.startsWith(`${prefix}/`)),
      `Unsafe archive member: ${name}`);
    return clean;
  });
  assert.equal(new Set(normalized).size, normalized.length, 'Duplicate archive member');
  const descriptions = tar(['-tvzf']).toString('utf8').trim().split('\n');
  assert.equal(descriptions.length, members.length, 'Ambiguous archive listing');
  assert.ok(descriptions.every(line => line.startsWith('-') || line.startsWith('d')), 'Archive links/special entries are forbidden');
  // Never extract archive paths into the filesystem. Read only fixed regular
  // members to stdout, then write known filenames into our private directory.
  // In particular, pobr-data.json.gz is never unpacked, decoded or staged.
  function member(name) {
    assert.ok(normalized.includes(`${prefix}/${name}`), `Missing package member: ${name}`);
    return execFileSync('tar', ['-xOzf', frozenArchive, `${prefix}/${name}`], { maxBuffer: 64 * 1024 * 1024, timeout: 60_000 });
  }
  const manifestBytes = member('manifest.json');
  const manifest = JSON.parse(manifestBytes);
  for (const [key, value] of Object.entries(manifest)) assert.deepEqual(report[key], value, `Package manifest/report mismatch: ${key}`);
  assert.ok(manifest.target === 'web' && typeof manifest.packageVersion === 'string'
    && /^[a-f0-9]{40}$/.test(manifest.commit) && manifest.build?.commit === manifest.commit
    && isHash(manifest.build.sourceFingerprint), 'Invalid package code identity');
  assert.equal(manifest.schemaVersion, expectedSchema, 'Package/current JSON schema mismatch');
  assert.equal(manifest.build.schemaVersion, expectedSchema, 'Package build/schema mismatch');
  const apiTypes = member('api-types.ts').toString('utf8');
  assert.equal(Number(apiTypes.match(/export const EXPECTED_SCHEMA_VERSION = (\d+);/)?.[1]), expectedSchema,
    'Packaged JSON contract/schema mismatch');
  const loaded = {};
  for (const name of ['pobr_wasm.js', 'pobr_wasm_bg.wasm']) {
    const entries = manifest.files?.filter(file => file.name === name);
    assert.equal(entries?.length, 1, `Missing/duplicate package runtime identity: ${name}`);
    const spec = entries[0];
    const bytes = member(name);
    assert.equal(bytes.length, spec.bytes, `Package runtime byte size mismatch: ${name}`);
    assert.ok(isHash(spec.sha256), `Invalid package runtime digest: ${name}`);
    assert.equal(sha256(bytes), spec.sha256, `Package runtime hash mismatch: ${name}`);
    assert.equal(manifest.build.files?.[name], spec.sha256, `Package build/runtime mismatch: ${name}`);
    loaded[name] = bytes;
  }
  writeFileSync(join(temp, 'pobr_wasm.mjs'), loaded['pobr_wasm.js'], { flag: 'wx' });
  writeFileSync(join(temp, 'pobr_wasm_bg.wasm'), loaded['pobr_wasm_bg.wasm'], { flag: 'wx' });
  return { manifest, manifest_sha256: sha256(manifestBytes), archive,
    assets: Object.fromEntries(sums), sums_sha256: sha256(inputs.read(join(root, 'SHA256SUMS'))),
    enginePath: join(temp, 'pobr_wasm.mjs'), wasm: loaded['pobr_wasm_bg.wasm'] };
}

function resultSummary(result, expectedRuntime, version, engineVersion) {
  assert.equal(result.provenance?.data_sha256, expectedRuntime, 'Calculation candidate provenance mismatch');
  assert.equal(result.provenance?.data_version, version, 'Calculation candidate data version mismatch');
  assert.equal(result.provenance?.engine_version, engineVersion, 'Calculation package engine version mismatch');
  assert.ok(Array.isArray(result.stats), 'Calculation stats are missing');
  const life = result.stats.find(stat => stat.id === 'Life')?.value;
  assert.ok(Number.isFinite(life) && life > 0, 'Expected a positive finite Life result');
  assert.ok(Array.isArray(result.unsupported_modifiers), 'Calculation dropped unsupported_modifiers diagnostics');
  assert.ok(Array.isArray(result.item_errors), 'Calculation dropped item_errors diagnostics');
  return { response_sha256: sha256(canonical(result)), provenance: result.provenance,
    stats: result.stats.filter(stat => ['Life', 'Mana', 'EnergyShield', 'TotalDPS'].includes(stat.id)),
    main_skill: result.main_skill, full_dps: result.full_dps,
    // Keep diagnostics themselves, including any nested skill/jewel diagnostics.
    unsupported_modifiers: result.unsupported_modifiers, item_errors: result.item_errors,
    ...(Object.hasOwn(result, 'missing_items') ? { missing_items: result.missing_items } : {}),
    tree_effects: result.tree_effects, minions: result.minions };
}

function publish(out, receipt) {
  assert.ok(noSymlinks(dirname(out)).isDirectory(), 'Receipt parent must already exist');
  assert.ok(!noSymlinks(out, true), 'Receipt output must be fresh');
  const temporary = join(dirname(out), `.runtime-candidate-receipt-${randomUUID()}.tmp`);
  writeFileSync(temporary, `${JSON.stringify(receipt, null, 2)}\n`, { flag: 'wx' });
  try {
    noSymlinks(dirname(out));
    linkSync(temporary, out); // Atomic, and unlike rename cannot overwrite a concurrent receipt.
  } finally {
    // Removing our private temporary name must not invalidate a published receipt.
    try { unlinkSync(temporary); } catch { /* The receipt's contents are already fixed. */ }
  }
}

export async function smokeRuntimeCandidate({ package: packageDir, data, expectedRuntime, out }) {
  assert.ok(isHash(expectedRuntime), '--expected-runtime must be a lowercase SHA256');
  const packageRoot = explicitPath(packageDir);
  const dataRoot = explicitPath(data);
  out = explicitPath(out);
  const within = (path, root) => path === root || path.startsWith(`${root}${sep}`);
  assert.ok(!within(packageRoot, dataRoot) && !within(dataRoot, packageRoot), 'Package and candidate inputs must be separate');
  assert.ok(!within(out, dataRoot) && !within(out, packageRoot), 'Receipt must be outside input directories');
  assert.ok(noSymlinks(dirname(out)).isDirectory(), 'Receipt parent must already exist');
  assert.ok(!noSymlinks(out, true), 'Receipt output must be fresh');
  const head = () => execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim();
  const initialHead = head();
  const packageDirectory = stamp(noSymlinks(packageRoot));
  const inputs = inputReader();
  const codeFiles = ['web/scripts/smoke-runtime-candidate.mjs', 'web/scripts/sync-data.mjs',
    'web/scripts/sync-build-references.mjs', 'web/src/api/types.ts'];
  const code = Object.fromEntries(codeFiles.map(name => [name, sha256(inputs.read(join(repo, name)))]));
  const workedRequests = workedBuilds.map(name => {
    const source = `examples/demo-bd-test/builds/${name}/code.txt`;
    const bytes = inputs.read(join(repo, source));
    const pob_code = bytes.toString('utf8').trim();
    assert.ok(pob_code.length, `Empty worked build: ${name}`);
    return { name, request: { pob_code, full_dps: true, breakdowns: [] }, input: { path: source, sha256: sha256(bytes) } };
  });
  const schema = Number(inputs.read(join(repo, 'web/src/api/types.ts')).toString('utf8')
    .match(/export const EXPECTED_SCHEMA_VERSION = (\d+);/)?.[1]);
  assert.ok(Number.isSafeInteger(schema) && schema > 0, 'Missing current JSON schema identity');
  const selected = candidate(dataRoot, expectedRuntime, inputs);
  const temp = mkdtempSync(join(realpathSync(tmpdir()), 'pobr-runtime-candidate-'));
  let tempExists = true;
  try {
    const packaged = standalone(packageRoot, temp, inputs, schema);
    const engine = await import(pathToFileURL(packaged.enginePath).href);
    engine.initSync({ module: packaged.wasm });
    assert.equal(engine.schemaVersion(), schema, 'Compiled WASM/JSON schema mismatch');
    assert.equal(engine.isDataReady(), false, 'Package engine must start without game data');
    for (const file of selected.staged) engine.stageDataFile(file.path, file.content);
    engine.initStagedData();
    assert.equal(engine.isDataReady(), true, 'Candidate data initialization failed');
    const calculations = [];
    const calculate = (name, request, input) => {
      const result = JSON.parse(engine.calculateBuildJson(JSON.stringify(request)));
      const summary = resultSummary(result, expectedRuntime, selected.version, packaged.manifest.packageVersion);
      calculations.push({ name, input, request_sha256: sha256(canonical(request)),
        request: { ...request, ...(request.pob_code ? { pob_code: { sha256: sha256(request.pob_code) } } : {}) }, result: summary });
      return result;
    };
    for (const { name, request, input } of workedRequests) calculate(name, request, input);
    // Reuse the shipped demo's small edit probe, without requiring diagnostics to be empty.
    const simple = { character: { class_name: 'Warrior', level: 90 },
      config_inputs: { 'questInterlude 2Khari CrossingMolten Shrine': false },
      extra_modifiers: [unsupportedSentinel], breakdowns: [] };
    const baseline = calculate('simple-character', simple);
    const editedRequest = { ...simple, extra_modifiers: [unsupportedSentinel, '+100 to maximum Life'] };
    const edited = calculate('simple-character-plus-life', editedRequest);
    const life = response => response.stats.find(stat => stat.id === 'Life').value;
    assert.equal(life(edited) - life(baseline), 100, 'Existing +100 Life edit probe failed');
    for (const response of [baseline, edited]) {
      assert.equal(response.unsupported_modifiers.filter(line => line === unsupportedSentinel).length, 1,
        'Synthetic unsupported sentinel lost its original extra_modifiers text');
    }
    const saved = engine.encodeBuildJson(JSON.stringify(editedRequest));
    assert.ok(typeof saved === 'string' && saved.length, 'Save returned no build code');
    const reopened = calculate('simple-character-save-reopen', { pob_code: saved, breakdowns: [] },
      { saved_from: 'simple-character-plus-life', sha256: sha256(saved) });
    // encodeBuildJson stores extra_modifiers in a custom block. The existing
    // API then attributes these lines to Config instead of the extra-text lane.
    const savedDiagnostics = edited.unsupported_modifiers.map(line => line === unsupportedSentinel ? savedSentinelDiagnostic : line);
    assert.deepEqual(reopened.unsupported_modifiers, savedDiagnostics, 'Save/reopen changed unsupported text/source diagnostics');
    assert.deepEqual(reopened.item_errors, edited.item_errors, 'Save/reopen changed item diagnostics');
    assert.equal(life(reopened), life(edited), 'Save/reopen changed the Life edit result');
    const receipt = { schema: 'pobr-runtime-candidate-smoke/v1', status: 'passed',
      code: { head: initialHead, files: code },
      inputs: { package: packageRoot, data: dataRoot, expected_runtime: expectedRuntime },
      package: { archive: packaged.archive, manifest_sha256: packaged.manifest_sha256,
        assets: packaged.assets, sums_sha256: packaged.sums_sha256,
        runtime_files: packaged.manifest.files.filter(file => ['pobr_wasm.js', 'pobr_wasm_bg.wasm'].includes(file.name)),
        package_version: packaged.manifest.packageVersion, schema_version: schema,
        commit: packaged.manifest.commit, build: packaged.manifest.build, bundled_data_used: false },
      candidate: { bundle_id: selected.bundle.bundle_id, bundle_sha256: sha256(inputs.read(join(dataRoot, 'bundle.json'))),
        current_sha256: sha256(inputs.read(join(dataRoot, 'data/CURRENT'))),
        runtime_identity: expectedRuntime, data_version: selected.version,
        staged_files: selected.staged.map(({ content, ...file }) => file) },
      calculations, edit_check: { modifier: '+100 to maximum Life', life_delta: 100 },
      diagnostic_check: { synthetic_modifier: unsupportedSentinel,
        direct: { source: 'extra_modifiers', expected_diagnostic: unsupportedSentinel },
        reopened: { source: 'Config', expected_diagnostic: savedSentinelDiagnostic },
        saved_code_sha256: sha256(saved), save_reopen_preserved: ['unsupported_modifiers (with Config source)', 'item_errors', 'Life'] },
      scope: 'Byte integrity, schema, candidate provenance, three worked-build API requests and the existing Life edit probe. '
        + 'Synthetic unsupported text/source and diagnostics checked across save/reopen; '
        + 'no numerical golden, accuracy certification, native comparison, browser test or CI-run attestation.' };
    inputs.verify();
    assert.deepEqual(inventory(dataRoot), selected.listing, 'Candidate input set changed during smoke');
    assert.equal(stamp(noSymlinks(packageRoot)), packageDirectory, 'Package directory changed during smoke');
    assert.equal(head(), initialHead, 'Source HEAD changed during smoke');
    rmSync(temp, { recursive: true, force: true });
    tempExists = false;
    publish(out, receipt);
    return receipt;
  } finally { if (tempExists) rmSync(temp, { recursive: true, force: true }); }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: { package: { type: 'string' }, data: { type: 'string' },
      'expected-runtime': { type: 'string' }, out: { type: 'string' } } });
    await smokeRuntimeCandidate({ ...values, expectedRuntime: values['expected-runtime'] });
    console.log(`Candidate runtime smoke passed; receipt: ${resolve(values.out)}`);
  } catch (error) {
    console.error(`Candidate runtime smoke failed: ${error.message}`);
    process.exitCode = 1;
  }
}
