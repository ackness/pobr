#!/usr/bin/env node
// Check the interface copy in src/locales/<lang>.json against en-US.json.
//   errors:   invalid JSON, a value that is not a string, a namespace or key
//             absent from en-US, a {placeholder} set that differs from
//             English, a file name that is not a language tag, a file that
//             src/locales/index.ts does not register (or a registered file
//             that is missing)
//   warnings: keys a language omits; they render in English
// Usage: node scripts/check-locales.mjs [locales dir]   (exit 1 on errors)
import { readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const SOURCE = 'en-US';
const PLACEHOLDER = /\{[A-Za-z0-9_]+\}/g;
const TAG = /^[a-z]{2,3}(?:-[A-Z][a-z]{3})?(?:-[A-Z]{2})?$/;
const placeholders = text => (text.match(PLACEHOLDER) ?? []).sort().join(' ');

function parse(file, errors) {
  let data;
  try {
    data = JSON.parse(readFileSync(file, 'utf8'));
  } catch (error) {
    errors.push(`${basename(file)}: invalid JSON (${error.message})`);
    return null;
  }
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    errors.push(`${basename(file)}: top level must be an object of namespaces`);
    return null;
  }
  const entries = new Map();
  for (const [ns, table] of Object.entries(data)) {
    if (!table || typeof table !== 'object' || Array.isArray(table)) {
      errors.push(`${basename(file)}: namespace ${ns} must be an object of strings`);
      continue;
    }
    for (const [key, value] of Object.entries(table)) {
      if (typeof value !== 'string') errors.push(`${basename(file)}: ${ns}/${key} is not a string`);
      else entries.set(`${ns}/${key}`, value);
    }
  }
  return entries;
}

/** Check every locale file in `dir`; `registry` is the source of index.ts. */
export function checkLocales(dir, registry = readFileSync(join(dir, 'index.ts'), 'utf8')) {
  const errors = [];
  const warnings = [];
  const files = readdirSync(dir).filter(name => name.endsWith('.json')).sort();
  const registered = new Set([...registry.matchAll(/from\s+['"]\.\/([^'"/]+\.json)['"]/g)].map(match => match[1]));
  for (const name of files) if (!registered.has(name)) errors.push(`${name}: not registered in index.ts`);
  for (const name of registered) if (!files.includes(name)) errors.push(`${name}: registered in index.ts but missing`);
  if (!files.includes(`${SOURCE}.json`)) return { errors: [...errors, `${SOURCE}.json is missing`], warnings, summary: [] };

  const source = parse(join(dir, `${SOURCE}.json`), errors);
  const summary = [];
  for (const name of files) {
    const lang = name.slice(0, -'.json'.length);
    if (!TAG.test(lang)) errors.push(`${name}: file name is not a language tag such as zh-CN`);
    const entries = lang === SOURCE ? source : parse(join(dir, name), errors);
    if (!entries || !source) continue;
    for (const [key, text] of entries) {
      if (!source.has(key)) errors.push(`${name}: ${key} is not in ${SOURCE}.json`);
      else if (placeholders(text) !== placeholders(source.get(key))) {
        errors.push(`${name}: ${key} placeholders [${placeholders(text)}] differ from ${SOURCE} [${placeholders(source.get(key))}]`);
      }
    }
    const missing = [...source.keys()].filter(key => !entries.has(key));
    if (missing.length) {
      const shown = missing.slice(0, 10).join(', ');
      warnings.push(`${name}: ${missing.length} keys fall back to ${SOURCE}: ${shown}${missing.length > 10 ? ', …' : ''}`);
    }
    summary.push({ lang, keys: entries.size, missing: missing.length });
  }
  return { errors, warnings, summary };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const dir = process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), '../src/locales');
  const { errors, warnings, summary } = checkLocales(dir);
  for (const { lang, keys, missing } of summary) console.log(`${lang}: ${keys} keys${missing ? `, ${missing} missing` : ''}`);
  for (const warning of warnings) console.warn(`warning: ${warning}`);
  for (const error of errors) console.error(`error: ${error}`);
  process.exitCode = errors.length ? 1 : 0;
}
