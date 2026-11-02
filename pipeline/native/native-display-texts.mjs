// Project the translation library's verified catalog into a small display-only
// sidecar. The library owns pairing and applicability; this adapter never joins
// physical rows, converts a locale, renders numeric placeholders, or changes facts.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath, pathToFileURL } from 'node:url';

const requireValue = (value, message) => { if (!value) throw new Error(message); };
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const sorted = entries => Object.fromEntries([...entries].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0));
const serialize = value => JSON.stringify(value, null, 2) + '\n';
const LIBRARY_FILES = ['src/native-catalog.mjs', 'src/native-names.mjs', 'src/strict-json.mjs'];
const BINDING_LIBRARY_FILES = [...LIBRARY_FILES, 'src/native-catalog-binding.mjs', 'src/native-localization.mjs'];
const FK_LIBRARY_FILES = [...BINDING_LIBRARY_FILES, 'src/native-catalog-fk-binding.mjs'];
// One optional closed module, pinned separately from the official catalog producer
// closure: it only adds reviewed-only supplement lookup, never a new official module.
const REVIEWED_MODULE = 'src/translation-workbench.mjs';
// Exact-ID readers currently live in web/src/lib/nativeDisplayNames.ts. Other
// fields can supply literal text, but do not need a second per-ID browser index.
const REQUIREMENTS_PATH = fileURLToPath(new URL('../runtime/data-requirements.json', import.meta.url));
const defaultProjection = () => JSON.parse(fs.readFileSync(REQUIREMENTS_PATH, 'utf8')).translation;
const strip = text => text.replace(/\[([^\[\]|]+)(?:\|([^\[\]]*))?\]/gu,
  (_, key, label) => label ?? key).trim();

export function projectDisplayTexts(catalog, lookup, catalogSha256, projection = defaultProjection()) {
  requireValue(projection.literals === 'all-catalog-conflict-participants'
    && Array.isArray(projection.scopes) && new Set(projection.scopes).size === projection.scopes.length
    && projection.scopes.every(scope => typeof scope === 'string' && /^[A-Za-z][A-Za-z0-9_]*\.[A-Za-z][A-Za-z0-9_]*$/u.test(scope)),
  'Invalid runtime projection');
  const runtimeScopes = new Set(projection.scopes);
  const scopes = new Map(), identities = new Set(), literalRows = new Map(), decisions = [];
  const add = (english, localized, identity) => {
    if (!literalRows.has(english)) literalRows.set(english, []);
    literalRows.get(english).push({ localized, identity });
  };
  for (const row of catalog.records) {
    // Exact scope, identity, current English and the field's complete English context:
    // the supplement lookup requires the declared realm/locale/patch and all three,
    // and the official lookup's own context filter keeps the same row.
    const query = { game_patch: catalog.game_patch, realm: catalog.realm, locale: catalog.locale,
      table: row.table, field: row.field, id: row.id, english: row.english ?? '',
      context: row.context?.english ?? null };
    const result = lookup(query);
    const localized = result.status === 'translated' ? result.text : null;
    const identity = [row.table, row.field, row.id];
    decisions.push({ identity, ...result });
    const scope = `${row.table}.${row.field}`;
    const key = JSON.stringify([scope, String(row.id)]);
    requireValue(!identities.has(key), 'Duplicate projected display identity');
    identities.add(key);
    if (runtimeScopes.has(scope)) {
      if (!scopes.has(scope)) scopes.set(scope, new Map());
      scopes.get(scope).set(String(row.id), [row.english, localized]);
    }
    // A rejected v2 group can have no single English value. Every candidate
    // still owns its literal and display alias, blocking other IDs and old
    // dictionaries from silently supplying a translation for that ambiguity.
    for (const english of new Set((row.candidates?.english ?? []).map(candidate => candidate.text))) {
      if (typeof english !== 'string' || !english.trim() || english === row.english) continue;
      add(english, null, identity);
      const alias = strip(english);
      if (alias && alias !== english) add(alias, null, identity);
    }
    if (!row.english?.trim()) continue;
    // Whole literal text only. A caller rendering numeric/implicit slots must
    // use the shared CSD renderer, never substitute captured display numbers here.
    if (!row.templates.english?.syntax_valid || row.templates.english.placeholders.length
      || row.templates.english.hash_slots) continue;
    add(row.english, localized, identity);
    // The ordinary Web display removes reference markup. Preserve the raw key
    // too, and consider collisions introduced by stripping rather than choosing
    // a row, a table, or the last translation.
    const english = strip(row.english);
    if (english && english !== row.english) add(english, localized === null ? null : strip(localized), identity);
  }
  const conflicts = [], entries = new Map();
  for (const [english, rows] of literalRows) {
    const values = new Set(rows.map(row => row.localized));
    const text = values.size === 1 && !values.has(null) ? rows[0].localized : null;
    entries.set(english, text);
    if (rows.length > 1 && text === null) conflicts.push({ english, rows });
  }
  return {
    sidecar: { schema: 'pobr-native-display-texts/v1', game_patch: catalog.game_patch,
      realm: catalog.realm, locale: catalog.locale, catalog_sha256: catalogSha256,
      entries: sorted(entries), scopes: sorted([...scopes].map(([key, rows]) => [key, sorted(rows)])) },
    decisions, conflicts,
  };
}

function readPinned(file, pin, reads) {
  requireValue(pin && /^[a-f0-9]{64}$/u.test(pin.sha256), 'Missing external SHA256');
  requireValue(pin.bytes === undefined || (Number.isSafeInteger(pin.bytes) && pin.bytes >= 0), 'Invalid declared input size');
  const absolute = path.resolve(file);
  requireValue(fs.realpathSync(absolute) === absolute, 'Input path contains a symlink');
  const fd = fs.openSync(absolute, fs.constants.O_RDONLY | fs.constants.O_NONBLOCK | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    requireValue(stat.isFile(), 'Input must be a regular file');
    requireValue(pin.bytes === undefined || stat.size === pin.bytes, 'Input size differs from manifest');
    const raw = fs.readFileSync(fd);
    requireValue(raw.length === stat.size, 'Input size changed while reading');
    requireValue(digest(raw) === pin.sha256, `Input SHA256 mismatch: ${absolute}`);
    reads.set(absolute, { sha256: pin.sha256, bytes: raw.length });
    return raw;
  } finally { fs.closeSync(fd); }
}

export async function createDisplayTexts(request, out) {
  requireValue(request.schema === 'pobr-native-display-texts-request/v1', 'Invalid display request');
  const reads = new Map();
  const manifest = JSON.parse(readPinned(request.catalog.manifest, request.catalog, reads));
  const version = new Map([
    ['poe2-native-catalog-manifest/v1', 'poe2-native-catalog/v1'],
    ['poe2-native-catalog-manifest/v2', 'poe2-native-catalog/v2'],
    ['poe2-native-catalog-manifest/v3', 'poe2-native-catalog/v3'],
  ]).get(manifest.schema);
  requireValue(version && manifest.status === 'candidate'
    && manifest.production_ready === false && manifest.game === 'poe2', 'Invalid catalog manifest');
  for (const field of ['game_patch', 'realm', 'locale']) requireValue(manifest[field] === request[field], `Catalog ${field} mismatch`);
  const fk = version === 'poe2-native-catalog/v3';
  const supplement = request.supplement;
  requireValue(!supplement || fk, 'Reviewed supplements require a v3 catalog');
  const payloads = ['catalog.json', 'request.json', ...(fk ? ['verification.json'] : [])];
  requireValue(JSON.stringify(Object.keys(manifest.files).sort()) === JSON.stringify(payloads.sort()), 'Unexpected catalog files');
  const bytes = readPinned(path.join(path.dirname(request.catalog.manifest), 'catalog.json'), manifest.files['catalog.json'], reads);
  readPinned(path.join(path.dirname(request.catalog.manifest), 'request.json'), manifest.files['request.json'], reads);
  // Execute only the exact already-reviewed library implementation, including its
  // local imports. The external request pins this closed module inventory too.
  const verification = fk ? readPinned(path.join(path.dirname(request.catalog.manifest), 'verification.json'),
    manifest.files['verification.json'], reads) : undefined;
  const libraries = fk ? [FK_LIBRARY_FILES] : version === 'poe2-native-catalog/v1'
    ? [LIBRARY_FILES, BINDING_LIBRARY_FILES, FK_LIBRARY_FILES] : [BINDING_LIBRARY_FILES, FK_LIBRARY_FILES];
  const libraryFiles = libraries.find(files => JSON.stringify(Object.keys(request.library.files).sort())
    === JSON.stringify([...files].sort()));
  requireValue(libraryFiles, 'Invalid library closure');
  if (fk) requireValue(JSON.stringify(sorted(Object.entries(manifest.producer ?? {})))
    === JSON.stringify(sorted(Object.entries(request.library.files))), 'Library producer closure mismatch');
  for (const file of libraryFiles) readPinned(path.join(request.library.root, file),
    { sha256: request.library.files[file] }, reads);
  // A reviewed supplement adds one separately pinned module and a compact package.
  // Its manifest producer must match the official module hashes exactly, plus that
  // module; nothing here joins the official catalog producer closure.
  let reviewed;
  if (supplement) {
    const module = supplement.module;
    requireValue(module?.file === REVIEWED_MODULE && /^[a-f0-9]{40}$/u.test(module.revision ?? '')
      && Number.isSafeInteger(module.bytes) && module.bytes >= 0,
      'Invalid supplement module pin');
    requireValue(Number.isSafeInteger(supplement.bytes) && supplement.bytes >= 0,
      'Invalid supplement manifest pin');
    readPinned(path.join(request.library.root, REVIEWED_MODULE), module, reads);
    const supplementManifest = JSON.parse(readPinned(supplement.manifest, supplement, reads));
    requireValue(supplementManifest.schema === 'poe2-reviewed-native-supplements-manifest/v1'
      && supplementManifest.status === 'candidate' && supplementManifest.production_ready === false
      && JSON.stringify(Object.keys(supplementManifest.files ?? {}).sort())
      === JSON.stringify(['request.json', 'supplements.json']), 'Invalid supplement manifest');
    const directory = path.dirname(supplement.manifest);
    readPinned(path.join(directory, 'request.json'), supplementManifest.files['request.json'], reads);
    const runtimeBytes = readPinned(path.join(directory, 'supplements.json'),
      supplementManifest.files['supplements.json'], reads);
    const producer = { ...Object.fromEntries(libraryFiles.map(file =>
      [path.basename(file), request.library.files[file]])), [path.basename(REVIEWED_MODULE)]: module.sha256 };
    requireValue(JSON.stringify(sorted(Object.entries(supplementManifest.producer ?? {})))
      === JSON.stringify(sorted(Object.entries(producer))), 'Supplement producer closure mismatch');
    reviewed = { module, runtimeBytes, runtimeSha256: supplementManifest.files['supplements.json'].sha256,
      manifest: { bytes: supplement.bytes, sha256: supplement.sha256 } };
  }
  const { strictJson } = await import(pathToFileURL(path.join(request.library.root, 'src/strict-json.mjs')));
  const catalog = strictJson(bytes);
  requireValue(catalog.schema === version, 'Catalog schema differs from manifest');
  if (fk) requireValue(catalog.fk_verification?.file === 'verification.json'
    && catalog.fk_verification.bytes === manifest.files['verification.json'].bytes
    && catalog.fk_verification.sha256 === manifest.files['verification.json'].sha256,
  'Catalog verification pin differs from manifest');
  // The supplement library validates the same original catalog/verification bytes
  // and keeps the official lookup as its fallback, so it is built only once here.
  const lookup = reviewed
    ? (await import(pathToFileURL(path.join(request.library.root, REVIEWED_MODULE)))).createReviewedSupplementLookup(
      [{ bytes, sha256: manifest.files['catalog.json'].sha256, verification }],
      { bytes: reviewed.runtimeBytes, sha256: reviewed.runtimeSha256 })
    : (await import(pathToFileURL(path.join(request.library.root, 'src/native-catalog.mjs')))).createCatalogLookup(
      catalog, { ...request, verification });
  if (reviewed) {
    const runtime = strictJson(reviewed.runtimeBytes);
    requireValue(runtime.schema === 'poe2-reviewed-native-supplements/v1' && runtime.status === 'candidate'
      && runtime.production_ready === false && Array.isArray(runtime.sources) && Array.isArray(runtime.records),
    'Invalid reviewed supplement runtime');
    reviewed.records = runtime.records.length;
    reviewed.sources = runtime.sources;
  }
  let projection = request.projection;
  if (!projection) {
    const raw = fs.readFileSync(REQUIREMENTS_PATH);
    readPinned(REQUIREMENTS_PATH, { bytes: raw.length, sha256: digest(raw) }, reads);
    projection = JSON.parse(raw).translation;
  }
  const projected = projectDisplayTexts(catalog, lookup, manifest.files['catalog.json'].sha256, projection);
  for (const [file, pin] of [...reads]) readPinned(file, pin, reads);
  const output = path.resolve(out), cache = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../.cache');
  requireValue(output.startsWith(cache + path.sep) && !fs.existsSync(output), 'Output must be a fresh repository cache candidate');
  requireValue(fs.realpathSync(path.dirname(output)) === path.dirname(output), 'Output parent contains a symlink');
  fs.mkdirSync(output);
  const raw = JSON.stringify(projected.sidecar) + '\n';
  fs.writeFileSync(path.join(output, 'display_texts.json'), raw, { flag: 'wx' });
  const receipt = { schema: 'pobr-native-display-texts-receipt/v1', adoption: 'not-adopted', request,
    inputs: Object.fromEntries(reads), files: { 'display_texts.json': { bytes: Buffer.byteLength(raw), sha256: digest(raw) } },
    implementation_sha256: digest(fs.readFileSync(fileURLToPath(import.meta.url))),
    decisions: projected.decisions, conflicts: projected.conflicts,
    runtime_scopes: [...projection.scopes].sort(),
    ...(reviewed ? { supplement: { manifest: reviewed.manifest, module: reviewed.module,
      runtime: { bytes: reviewed.runtimeBytes.length, sha256: reviewed.runtimeSha256 },
      catalog_sha256: manifest.files['catalog.json'].sha256, sources: reviewed.sources,
      records: reviewed.records } } : {}),
    scope: 'Only declared runtime scopes retain browser per-ID indexes. Every source record still participates in exact library decisions and applicable literal collision checks, including unresolved records and markup aliases. Null blocks stale compatibility text. Other scoped metadata stays in the translation library. No cross-locale conversion, placeholder rendering, calculation, source approval or CURRENT adoption.' };
  fs.writeFileSync(path.join(output, 'receipt.json'), serialize(receipt), { flag: 'wx' });
  return receipt;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [flag, file, outFlag, out] = process.argv.slice(2);
  requireValue(flag === '--request' && outFlag === '--out' && out && process.argv.length === 6, 'Usage: --request file.json --out .cache/fresh');
  const receipt = await createDisplayTexts(JSON.parse(fs.readFileSync(file, 'utf8')), out);
  console.log(JSON.stringify({ files: receipt.files, records: receipt.decisions.length, conflicts: receipt.conflicts.length }));
}
