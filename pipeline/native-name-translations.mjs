// Adapt the independent translation library's pinned name pairs to existing DTO sidecars.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const required = (ok, message) => { if (!ok) throw new Error(message); };
const digest = raw => createHash('sha256').update(raw).digest('hex');
const domains = { BaseItemTypes: ['Name', 'base_items.json'], ActiveSkills: ['DisplayedName', 'skills.json'], Mods: ['Name', 'mods.json'] };
const nameMap = value => value && !Array.isArray(value) && typeof value === 'object'
  && Object.values(value).every(v => typeof v === 'string');

export function nativeNameSidecars(candidate, legacy, version, locale) {
  required(candidate.schema === 'poe2-native-names/v1' && candidate.status === 'candidate'
    && candidate.production_ready === false && candidate.game === 'poe2'
    && candidate.game_patch === version && candidate.realm === 'international'
    && candidate.locale === locale && locale === 'zh-TW'
    && candidate.language === 'Traditional Chinese', 'Name source identity/locale mismatch');
  required(Array.isArray(candidate.entries) && Array.isArray(candidate.excluded), 'Missing name inventory');
  const files = {}, records = {};
  for (const [table, [field, file]] of Object.entries(domains)) {
    required(nameMap(legacy[file]), `Invalid legacy name map: ${file}`);
    const output = new Map(Object.entries(legacy[file]));
    const seen = new Set(), englishRows = new Set(), localizedRows = new Set(), adopted = [], removed = [];
    for (const [rows, translated] of [[candidate.entries, true], [candidate.excluded, false]]) {
      for (const entry of rows.filter(e => e.table === table)) {
        required(entry.key_field === 'Id' && entry.field === field && typeof entry.id === 'string'
          && entry.id.length > 0 && !seen.has(entry.id), `Invalid/duplicate name identity: ${table}`);
        seen.add(entry.id);
        for (const [field, seenRows] of [['english_row', englishRows], ['localized_row', localizedRows]]) {
          const row = entry[field];
          required(row === null || Number.isInteger(row) && row >= 0 && !seenRows.has(row), 'Invalid/duplicate physical name row');
          if (row !== null) seenRows.add(row);
        }
        if (translated) {
          required(typeof entry.english === 'string' && entry.english.length > 0
            && typeof entry.localized === 'string' && entry.localized.length > 0
            && entry.english !== entry.localized && Number.isInteger(entry.english_row)
            && entry.english_row >= 0 && Number.isInteger(entry.localized_row) && entry.localized_row >= 0,
          'Invalid paired text');
          output.set(entry.id, entry.localized); adopted.push(entry);
        } else {
          required(['missing-localized-id', 'missing-english-id', 'empty-english-text', 'empty-localized-text', 'unchanged-text'].includes(entry.reason), 'Unknown excluded name reason');
          // A current source row without a translation must not resurrect a stale
          // old localized value under the same ID. Unknown historical IDs remain explicit compatibility.
          const prior = output.get(entry.id); output.delete(entry.id);
          removed.push({ ...entry, legacy_value: prior ?? null });
        }
      }
    }
    const counts = candidate.coverage?.[table];
    required(counts && adopted.length === counts.paired
      && englishRows.size === counts.english_rows && localizedRows.size === counts.localized_rows
      && [...englishRows].every(r => r < counts.english_rows)
      && [...localizedRows].every(r => r < counts.localized_rows), 'Incomplete name inventory');
    files[file] = Object.fromEntries([...output].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0));
    records[file] = { table, field, adopted, removed,
      historical_compatibility: Object.entries(legacy[file]).filter(([id]) => !seen.has(id)) };
  }
  return { files, report: { records, sources: candidate.sources,
    library_producer: candidate.producer, scope: 'Three existing ID-keyed zh-TW sidecars only; target translations override by exact ID. Current missing/unchanged text removes stale translation; unknown historical IDs retain pinned compatibility. Other locales, game facts and calculations are unchanged.' } };
}

function pinned(file, expected) {
  required(/^[a-f0-9]{64}$/u.test(expected), 'Invalid external SHA256');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NONBLOCK | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    required(stat.isFile() && stat.size <= 128 * 1024 * 1024, 'Expected bounded regular file');
    const raw = fs.readFileSync(fd);
    required(digest(raw) === expected, 'Name input SHA256 mismatch');
    return JSON.parse(raw.toString('utf8'));
  } finally { fs.closeSync(fd); }
}

export function createNameSidecars(request, out) {
  required(request.schema === 'pobr-native-name-request/v1', 'Invalid request');
  const candidate = pinned(request.native.path, request.native.sha256);
  const legacy = Object.fromEntries(Object.values(domains).map(([, file]) => [file,
    pinned(request.legacy[file].path, request.legacy[file].sha256)]));
  const result = nativeNameSidecars(candidate, legacy, request.version, request.locale);
  const output = path.resolve(out), cache = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../.cache');
  required(output.startsWith(cache + path.sep), 'Output must be a fresh repository cache candidate');
  required(!fs.existsSync(output), 'Output already exists');
  const parent = path.dirname(output);
  required(fs.realpathSync(parent) === parent, 'Output parent contains symlink');
  fs.mkdirSync(output);
  const files = {};
  for (const [file, value] of Object.entries(result.files)) {
    const raw = JSON.stringify(value, null, 2) + '\n';
    fs.writeFileSync(path.join(output, file), raw, { flag: 'wx' });
    files[file] = { bytes: Buffer.byteLength(raw), sha256: digest(raw) };
  }
  const receipt = { schema: 'pobr-native-name-receipt/v1', ...result.report, inputs: request,
    files, implementation_sha256: digest(fs.readFileSync(fileURLToPath(import.meta.url))), adoption: 'not-adopted' };
  fs.writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  return receipt;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [flag, file, outFlag, out] = process.argv.slice(2);
  required(flag === '--request' && outFlag === '--out' && out && process.argv.length === 6, 'Usage: --request file.json --out .cache/fresh');
  const result = createNameSidecars(JSON.parse(fs.readFileSync(file, 'utf8')), out);
  console.log(JSON.stringify(Object.fromEntries(Object.entries(result.records).map(([k, v]) => [k,
    { adopted: v.adopted.length, removed: v.removed.length, historical: v.historical_compatibility.length }]))));
}
