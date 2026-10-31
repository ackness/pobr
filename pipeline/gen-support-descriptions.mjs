#!/usr/bin/env node
// Display-only companion to gen-zh-cn: never rewrites the runtime snapshot or gem IDs.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { DICTIONARY_REPOSITORY, dictionarySource } from './dictionary-source.mjs';

const sourceFile = 'tables/GemEffects.json';
const plain = text => text.replace(/\[([^\]|]*\|)?([^\]]+)\]/g, '$2').trim();
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

export function supportDescriptions(dictionary, gems) {
  if (dictionary.table !== 'GemEffects' || !Array.isArray(dictionary.entries)) throw new Error('Expected GemEffects dictionary');
  const byEnglish = new Map();
  for (const entry of dictionary.entries) {
    for (const pair of entry.columns?.SupportText ?? []) {
      if (typeof pair.en !== 'string' || typeof pair.zh !== 'string') continue;
      const en = plain(pair.en), zh = plain(pair.zh);
      if (!en || !/\p{Script=Han}/u.test(zh)) continue;
      if (byEnglish.has(en) && byEnglish.get(en) !== zh) throw new Error(`Conflicting support description: ${en}`);
      byEnglish.set(en, zh);
    }
  }
  const entries = {}, untranslated = [];
  for (const gem of [...gems].filter(gem => gem.is_support).sort((a, b) => a.skill_id < b.skill_id ? -1 : a.skill_id > b.skill_id ? 1 : 0)) {
    if (Object.hasOwn(entries, gem.skill_id) || untranslated.includes(gem.skill_id)) throw new Error(`Duplicate support ID: ${gem.skill_id}`);
    const en = gem.description?.trim();
    const zh = byEnglish.get(en);
    if (zh) entries[gem.skill_id] = { en, zh_cn: zh };
    else untranslated.push(gem.skill_id);
  }
  return { entries, untranslated };
}

async function main() {
  if (process.argv.slice(2).some(arg => arg !== '--check')) throw new Error('Usage: gen-support-descriptions.mjs [--check]');
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const version = (await fs.readFile(path.join(root, 'data/CURRENT'), 'utf8')).trim();
  if (!/^[0-9]+(?:\.[0-9]+)+$/.test(version)) throw new Error('Invalid data version');
  const data = path.join(root, 'data', version);
  const meta = JSON.parse(await fs.readFile(path.join(data, 'i18n/zh-CN/_meta.json'), 'utf8'));
  if (meta.source !== `https://github.com/${DICTIONARY_REPOSITORY}`) throw new Error('Unexpected Chinese dictionary source');
  if (!/^[a-f0-9]{40}$/.test(meta.source_commit ?? '')) throw new Error('Expected a pinned Chinese dictionary commit');
  const source = await dictionarySource({ cacheRoot: path.join(root, 'pipeline/.cache/support-descriptions'),
    files: [sourceFile], ref: meta.source_commit });
  const bytes = await fs.readFile(path.join(source.directory, sourceFile));
  const catalog = await fs.readFile(path.join(data, 'overlay/trade_catalog.json'));
  const result = supportDescriptions(JSON.parse(bytes), JSON.parse(catalog).gems);
  const output = JSON.stringify({ _meta: {
    source: meta.source, source_commit: source.ref, source_file: sourceFile, source_sha256: sha256(bytes),
    data_version: version, catalog_sha256: sha256(catalog),
    regen_command: 'mise exec -- node pipeline/gen-support-descriptions.mjs',
  }, ...result }, null, 2) + '\n';
  const destination = path.join(root, 'web/src/lib/supportDescriptions.zh-CN.json');
  if (process.argv.includes('--check')) {
    if (await fs.readFile(destination, 'utf8') !== output) throw new Error('Support descriptions need regeneration');
  } else await fs.writeFile(destination, output);
  process.stdout.write(`${Object.keys(result.entries).length} translated supports; ${result.untranslated.length} English/unavailable fallbacks\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
