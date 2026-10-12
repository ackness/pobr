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
  const file = `data/snapshots/${snapshot}/generated/pobr.json`;
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

/** Preserve legacy and reviewed curated pairs; admit only unambiguous new automatic buckets. */
export function mergeTranslations(legacy, bundle) {
  validateBundle(bundle, bundle.snapshot);
  const pairs = [];
  const seen = new Set();
  const existingEnglish = new Set();
  const existingChinese = new Set();
  function add(row) {
    const key = pairKey(row);
    if (seen.has(key)) return false;
    seen.add(key);
    pairs.push({ src: row.src, en: row.en });
    existingEnglish.add(skeleton(row.en));
    existingChinese.add(skeleton(row.src));
    return true;
  }
  for (const row of legacy) add(row);
  const report = { legacy_pairs: pairs.length, curated_templates: 0, added_curated: 0,
    added_trade: 0, duplicates: 0, excluded: [], export_excluded_count: bundle.excluded.length };
  for (const row of bundle.templates.filter(row => row.kind === 'curated')) {
    report.curated_templates++;
    if (add(row)) report.added_curated++;
    else report.duplicates++;
  }
  const automatic = new Map(bundle.templates.filter(row => row.kind === 'trade').map(row => [pairKey(row), row]));
  const english = new Map(), chinese = new Map();
  for (const [key, row] of automatic) {
    for (const [index, value] of [[english, row.en], [chinese, row.src]]) {
      const bucket = skeleton(value);
      if (!index.has(bucket)) index.set(bucket, new Set());
      index.get(bucket).add(key);
    }
  }
  for (const [key, row] of automatic) {
    if (seen.has(key)) { report.duplicates++; continue; }
    const en = skeleton(row.en), cn = skeleton(row.src);
    const reason = existingEnglish.has(en) ? 'existing-english-template'
      : existingChinese.has(cn) ? 'existing-chinese-template'
      : english.get(en).size !== 1 || chinese.get(cn).size !== 1 ? 'automatic-skeleton-ambiguity' : null;
    if (reason) report.excluded.push({ reason, ...row });
    else { add(row); report.added_trade++; }
  }
  pairs.sort((a, b) => compare(a.src, b.src) || compare(a.en, b.en));
  report.excluded.sort((a, b) => compare(pairKey(a), pairKey(b)));
  return { pairs, report };
}
