import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { dictionarySource } from './dictionary-source.mjs';

export const TRANSLATION_REPOSITORY = 'ackness/poe2-zh-cn-stats';
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
const pairKey = row => JSON.stringify([row.src, row.en]);

function slots(template) {
  const indexes = new Set();
  const rest = template.replace(/\{(\d+)(?::[^{}]+)?\}/g, (_, index) => {
    indexes.add(Number(index));
    return '';
  });
  if (/[{}]/.test(rest)) throw new Error('Unsupported indexed translation placeholder');
  return JSON.stringify([...indexes].sort((a, b) => a - b));
}

function validateBundle(bundle, snapshot) {
  if (bundle?.schema_version !== 1 || bundle.snapshot !== snapshot || !Array.isArray(bundle.templates)
      || !Array.isArray(bundle.words) || !Array.isArray(bundle.excluded)) {
    throw new Error('Invalid or inconsistent PoBR translation export');
  }
  for (const row of bundle.templates) {
    if (!row || !['trade', 'curated'].includes(row.kind) || typeof row.src !== 'string' || !row.src.trim()
        || typeof row.en !== 'string' || !row.en.trim() || !/\p{Script=Han}/u.test(row.src)
        || !row.provenance || typeof row.provenance !== 'object' || Array.isArray(row.provenance)
        || slots(row.src) !== slots(row.en)) throw new Error('Invalid PoBR translation template');
  }
  const words = new Set();
  for (const row of bundle.words) {
    if (!row || typeof row.english !== 'string' || !/^[A-Z][a-zA-Z']*$/.test(row.english)
        || typeof row.chinese !== 'string' || !/\p{Script=Han}/u.test(row.chinese)
        || typeof row.note !== 'string' || !row.note.trim() || typeof row.source_url !== 'string'
        || !row.source_url.startsWith('https://') || words.has(row.english)) {
      throw new Error('Invalid or duplicate translated rare-name word');
    }
    words.add(row.english);
  }
}

/** Resolve the pointer and export from the same immutable commit, or an explicit local checkout. */
export async function translationSource({ cacheRoot, directory, ref, refresh = false, fetcher = fetch }) {
  if (directory && (ref || refresh)) throw new Error('Choose a local translation directory or a remote translation ref');
  let pointerRoot = directory;
  if (!directory) {
    const index = await dictionarySource({ cacheRoot: path.join(cacheRoot, 'index'), files: ['data/CURRENT'],
      ref, refresh, repository: TRANSLATION_REPOSITORY, prefix: '', fetcher });
    pointerRoot = index.directory;
    ref = index.ref;
  }
  const snapshot = (await fs.readFile(path.join(pointerRoot, 'data/CURRENT'), 'utf8')).trim();
  if (!/^[0-9TZ]+$/.test(snapshot)) throw new Error('Invalid translation snapshot pointer');
  const file = `data/snapshots/${snapshot}/generated/templates.json`;
  const exportRoot = directory ?? (await dictionarySource({ cacheRoot: path.join(cacheRoot, 'exports'),
    files: [file], ref, repository: TRANSLATION_REPOSITORY, prefix: '', fetcher })).directory;
  const text = await fs.readFile(path.join(exportRoot, file), 'utf8');
  const bundle = JSON.parse(text);
  validateBundle(bundle, snapshot);
  return { bundle, receipt: { repository: `https://github.com/${TRANSLATION_REPOSITORY}`,
    commit: directory ? null : ref, snapshot, sha256: createHash('sha256').update(text).digest('hex'),
    mode: directory ? 'local' : 'pinned' } };
}

// Match the conservative bucket used by apps/pobr-wasm/src/zh.rs. A bucket
// collision is not proof of equivalent mechanics, so new automatic text loses.
function skeleton(template) {
  return template.replace(/\{[^{}]*\}/g, '').replace(/[0-9+\-.,\s]/gu, '')
    .replace(/[A-Z]/g, letter => letter.toLowerCase());
}

const placeholder = /\{(\d+)(?::[^{}]+)?\}/g;
const plain = template => template.replace(placeholder, '{$1}').replace(/\s+/gu, ' ').trim();
const sameEnglish = (a, b) => plain(a).toLowerCase() === plain(b).toLowerCase();
const sameChinese = (a, b) => plain(a).replace(/\s/gu, '') === plain(b).replace(/\s/gu, '');

// Keep the legacy English template and each slot's legacy format (sign display,
// string capture); only the Chinese wording comes from the official text.
function officialWording(legacy, official) {
  const tokens = new Map([...legacy.src.matchAll(placeholder)].map(match => [match[1], match[0]]));
  if (slots(legacy.src) !== slots(official.src)) return null;
  return official.src.replace(placeholder, (_, index) => tokens.get(index));
}

/**
 * Preserve legacy templates and reviewed curated pairs; admit unambiguous new
 * automatic buckets. Where a legacy template has the same English as an
 * unambiguous official one but different Chinese, the official wording wins.
 */
export function mergeTranslations(legacy, bundle) {
  validateBundle(bundle, bundle.snapshot);
  const pairs = new Map();
  const byEnglish = new Map(), byChinese = new Map();
  const legacyKeys = new Set();
  const bucket = (index, value) => index.get(skeleton(value)) ?? new Set();
  function add(row) {
    const key = pairKey(row);
    if (pairs.has(key)) return false;
    pairs.set(key, { src: row.src, en: row.en });
    for (const [index, value] of [[byEnglish, row.en], [byChinese, row.src]]) {
      const name = skeleton(value);
      if (!index.has(name)) index.set(name, new Set());
      index.get(name).add(key);
    }
    return true;
  }
  function remove(key) {
    const row = pairs.get(key);
    pairs.delete(key);
    byEnglish.get(skeleton(row.en)).delete(key);
    byChinese.get(skeleton(row.src)).delete(key);
  }
  for (const row of legacy) if (add(row)) legacyKeys.add(pairKey(row));
  const report = { legacy_pairs: pairs.size, curated_templates: 0, added_curated: 0,
    added_trade: 0, replaced_legacy: [], duplicates: 0, excluded: [],
    export_excluded_count: bundle.excluded.length };
  for (const row of bundle.templates.filter(row => row.kind === 'curated')) {
    report.curated_templates++;
    if (add(row)) report.added_curated++;
    else { report.duplicates++; legacyKeys.delete(pairKey(row)); }
  }
  const automatic = new Map(bundle.templates.filter(row => row.kind === 'trade').map(row => [pairKey(row), row]));
  const english = new Map(), chinese = new Map();
  for (const [key, row] of automatic) {
    for (const [index, value] of [[english, row.en], [chinese, row.src]]) {
      const name = skeleton(value);
      if (!index.has(name)) index.set(name, new Set());
      index.get(name).add(key);
    }
  }
  const replacements = [];
  for (const [key, row] of automatic) {
    if (pairs.has(key)) { report.duplicates++; continue; }
    const en = skeleton(row.en), cn = skeleton(row.src);
    const unambiguous = english.get(en).size === 1 && chinese.get(cn).size === 1;
    const sameTemplate = [...bucket(byEnglish, row.en)].filter(existing => sameEnglish(pairs.get(existing).en, row.en));
    if (bucket(byEnglish, row.en).size) {
      // Curated, multi-line and same-wording legacy templates are kept as they are.
      const targets = unambiguous ? sameTemplate.filter(existing => {
        const old = pairs.get(existing);
        return legacyKeys.has(existing) && !/\n/.test(old.src + old.en) && !sameChinese(old.src, row.src)
          && officialWording(old, row) !== null;
      }) : [];
      const others = new Set([...bucket(byChinese, row.src)].filter(existing => !targets.includes(existing)));
      if (targets.length && targets.length === sameTemplate.length && !others.size) replacements.push({ row, targets });
      else report.excluded.push({ reason: 'existing-english-template', ...row });
      continue;
    }
    const reason = bucket(byChinese, row.src).size ? 'existing-chinese-template'
      : !unambiguous ? 'automatic-skeleton-ambiguity' : null;
    if (reason) report.excluded.push({ reason, ...row });
    else { add(row); report.added_trade++; }
  }
  for (const { row, targets } of replacements) {
    for (const key of targets) {
      const old = pairs.get(key);
      const src = officialWording(old, row);
      remove(key);
      add({ src, en: old.en });
      report.replaced_legacy.push({ en: old.en, legacy: old.src, official: src, provenance: row.provenance });
    }
  }
  const merged = [...pairs.values()].sort((a, b) => compare(a.src, b.src) || compare(a.en, b.en));
  report.excluded.sort((a, b) => compare(pairKey(a), pairKey(b)));
  report.replaced_legacy.sort((a, b) => compare(a.en, b.en) || compare(a.legacy, b.legacy));
  return { pairs: merged, report };
}
