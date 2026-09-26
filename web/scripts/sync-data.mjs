// Publish a complete browser data directory from the runtime snapshot contract.
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { syncBuildReferences } from './sync-build-references.mjs';

const webRoot = fileURLToPath(new URL('..', import.meta.url));
const repoRoot = join(webRoot, '..');
const maintenanceFiles = new Set([
  'generated/modifier-audit.json', 'generated/parsed_mods.json',
  'generated/parse-coverage.json', 'generated/test_pins.json', 'overlay/stat_id_map.json',
]);

function walkJson(dir) {
  const out = [];
  if (!statSync(dir, { throwIfNoEntry: false })?.isDirectory()) return out;
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...walkJson(path));
    else if (entry.endsWith('.json')) out.push(path);
  }
  return out;
}

function safePath(name) {
  if (typeof name !== 'string' || /[\\:]/.test(name) || name.split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`Invalid snapshot path: ${name}`);
  }
  return name;
}

export function snapshotFiles({ dataRoot = join(repoRoot, 'data'), version } = {}) {
  version ??= process.env.POBR_DATA_VERSION ?? readFileSync(join(dataRoot, 'CURRENT'), 'utf8').trim();
  if (!/^[0-9]+(?:\.[0-9]+)+$/.test(version)) throw new Error('Invalid data version');
  const src = join(dataRoot, version);
  const manifest = JSON.parse(readFileSync(join(src, 'manifest.json'), 'utf8'));
  const copies = new Map();
  if (manifest.schema_version === 3) {
    const strings = (value) => Array.isArray(value) && value.every((entry) => typeof entry === 'string');
    const domains = Array.isArray(manifest.domains) ? { base: manifest.domains } : manifest.domains;
    if (manifest.poe_version !== version || !manifest.files || Array.isArray(manifest.files)
      || typeof manifest.files !== 'object' || Object.keys(manifest.files).length === 0
      || !strings(manifest.languages) || !domains || !strings(domains.base)
      || !strings(domains.overlay ?? []) || !strings(domains.generated ?? [])) {
      throw new Error('Invalid runtime snapshot manifest');
    }
    for (const [name, expected] of Object.entries(manifest.files)) {
      const rel = safePath(name);
      if (!['base', 'overlay', 'generated', 'i18n'].includes(rel.split('/')[0]) || !rel.endsWith('.json')) {
        throw new Error(`Invalid sealed snapshot path: ${rel}`);
      }
      const path = join(src, rel);
      const actual = createHash('sha256').update(readFileSync(path)).digest('hex');
      if (actual !== expected) throw new Error(`Snapshot fingerprint mismatch: ${rel}`);
      copies.set(rel, path);
    }
    for (const section of ['base', 'overlay', 'generated']) {
      for (const domain of domains[section] ?? []) {
        if (!copies.has(`${section}/${domain}.json`)) {
          throw new Error(`Declared domain lacks an inventory entry: ${section}/${domain}`);
        }
      }
    }
    for (const language of manifest.languages) {
      if (![...copies.keys()].some((name) => name.startsWith(`i18n/${language}/`))) {
        throw new Error(`Declared language lacks files: ${language}`);
      }
    }
  } else if (manifest.schema_version === 1 || manifest.schema_version === 2) {
    for (const path of walkJson(src)) {
      const rel = relative(src, path).split('\\').join('/');
      if (!maintenanceFiles.has(rel)) copies.set(rel, path);
    }
  } else throw new Error(`Unsupported data schema: ${manifest.schema_version}`);
  copies.set('manifest.json', join(src, 'manifest.json'));
  // User patches and shared curation preserve the runtime loader's layer semantics.
  for (const [directory, prefix] of [[join(src, 'patch'), 'patch'], [join(dataRoot, 'overlay-common'), 'overlay-common']]) {
    for (const path of walkJson(directory)) copies.set(`${prefix}/${relative(directory, path).split('\\').join('/')}`, path);
  }
  return { version, copies };
}

export function syncData({ dataRoot = join(repoRoot, 'data'), destRoot = join(webRoot, 'public', 'data'), version } = {}, rename = renameSync) {
  const snapshot = snapshotFiles({ dataRoot, version });
  version = snapshot.version;
  const copies = snapshot.copies;
  const files = [...copies.keys()].sort();
  mkdirSync(dirname(destRoot), { recursive: true });
  const staging = mkdtempSync(join(dirname(destRoot), '.data-sync-'));
  const stagedData = join(staging, 'data');
  const previous = join(staging, 'previous');
  let movedPrevious = false;
  let preserveStaging = false;
  try {
    for (const [rel, path] of copies) {
      const destination = join(stagedData, version, rel);
      mkdirSync(dirname(destination), { recursive: true });
      cpSync(path, destination);
    }
    writeFileSync(join(stagedData, 'manifest.json'), JSON.stringify({ version, files }, null, 2));
    if (statSync(destRoot, { throwIfNoEntry: false })) {
      rename(destRoot, previous);
      movedPrevious = true;
    }
    try { rename(stagedData, destRoot); }
    catch (error) {
      if (movedPrevious) {
        try { rename(previous, destRoot); }
        catch (rollbackError) {
          preserveStaging = true;
          throw new Error(`Browser data rollback failed; original backup retained at ${previous}`, { cause: rollbackError });
        }
      }
      throw error;
    }
  } finally { if (!preserveStaging) rmSync(staging, { recursive: true, force: true }); }
  return { version, files };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { version, files } = syncData();
  console.log(`synced ${files.length} runtime files from data/${version}`);
  syncBuildReferences();
}
