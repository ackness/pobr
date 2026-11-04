#!/usr/bin/env node
// One-shot migration: read the hard-coded UI dictionaries in TypeScript and
// write one JSON file per interface language to src/locales/. Every table is a
// plain object/array literal; it is cut out by bracket matching and evaluated
// as JavaScript, so no value is re-typed by hand. Run from web/:
//   node scripts/split-ui-locales.mjs
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const web = join(dirname(fileURLToPath(import.meta.url)), '..');
const LANGS = ['en-US', 'zh-CN', 'zh-TW'];

// shape:
//   entry  { key: { 'en-US': s, 'zh-TW': s, 'zh-CN': s } }
//   tuple  { key: [s, s, s] } with the accessor's index order
//   byLang { 'en-US': { key: s }, ... }
const TABLES = [
  ['ui', 'src/lib/i18n.ts', 'DICT', 'entry'],
  ['bootMessage', 'src/lib/i18n.ts', 'BOOT_MESSAGES', 'entry'],
  ['slot', 'src/lib/i18n.ts', 'SLOT_LABELS', 'entry'],
  ['configSection', 'src/lib/i18n.ts', 'CONFIG_SECTION_LABELS', 'entry'],
  ['enemyTier', 'src/lib/i18n.ts', 'ENEMY_TIER_LABELS', 'entry'],
  ['modName', 'src/lib/i18n.ts', 'MOD_NAME_LABELS', 'entry'],
  ['originKind', 'src/lib/i18n.ts', 'ORIGIN_KIND_LABELS', 'entry'],
  ['modType', 'src/lib/i18n.ts', 'MOD_TYPE_LABELS', 'entry'],
  ['statExtra', 'src/lib/i18n.ts', 'STAT_EXTRA_LABELS', 'entry'],
  ['damageType', 'src/lib/i18n.ts', 'DAMAGE_TYPE_LABELS', 'entry'],
  ['statCategory', 'src/lib/i18n.ts', 'STAT_CATEGORY_LABELS', 'entry'],
  ['detail', 'src/lib/detailLabels.ts', 'DETAIL_LABELS', 'entry'],
  ['support', 'src/lib/supportI18n.ts', 'messages', 'tuple', ['en-US', 'zh-CN', 'zh-TW']],
  ['guidance', 'src/lib/guidanceText.ts', 'text', 'tuple', ['en-US', 'zh-TW', 'zh-CN']],
  ['uniqueCatalog', 'src/lib/uniqueCatalogText.ts', 'labels', 'tuple', ['en-US', 'zh-TW', 'zh-CN']],
  ['itemCrafting', 'src/lib/itemCraftingText.ts', 'labels', 'tuple', ['en-US', 'zh-TW', 'zh-CN']],
  ['tradeAnalysis', 'src/lib/tradeAnalysisText.ts', 'messages', 'entry'],
  ['marketAffix', 'src/lib/marketAffixText.ts', 'labels', 'tuple', ['en-US', 'zh-TW', 'zh-CN']],
  ['upgrade', 'src/lib/upgradeText.ts', 'labels', 'tuple', ['en-US', 'zh-TW', 'zh-CN']],
  ['workspace', 'src/lib/workspaceText.ts', 'text', 'byLang'],
  ['calcs', 'src/components/calcs/calcsLabels.ts', 'labels', 'tuple', ['en-US', 'zh-CN', 'zh-TW']],
  ['configWorkflow', 'src/components/config/workflowLabels.ts', 'labels', 'tuple', ['en-US', 'zh-CN', 'zh-TW']],
  ['nodePower', 'src/components/tree/nodePowerLabels.ts', 'powerLabels', 'byLang', null, ['en']],
  ['appDialog', 'src/components/shared/AppDialogs.tsx', 'labels', 'byLang'],
  ['jewelPlanner', 'src/components/tree/JewelPassivePlanner.tsx', 'COPY', 'byLang'],
  ['treeOptimizer', 'src/components/tree/TreeOptimizer.tsx', 'COPY', 'byLang'],
  ['pageName', 'src/app/text.ts', 'pageNames', 'byLang'],
];

/** Return the source text of the literal assigned to `const name`. */
function literal(source, name, file) {
  const declaration = new RegExp(`(?:^|\\n)(?:export\\s+)?const\\s+${name}\\b[^=]*=\\s*`).exec(source);
  if (!declaration) throw new Error(`${file}: const ${name} not found`);
  let i = declaration.index + declaration[0].length;
  const start = i;
  if (!'{['.includes(source[i])) throw new Error(`${file}: ${name} is not an object/array literal`);
  let depth = 0;
  for (; i < source.length; i++) {
    const c = source[i];
    if (c === '"' || c === "'" || c === '`') {
      if (c === '`') throw new Error(`${file}: ${name} contains a template literal`);
      for (i++; source[i] !== c; i++) if (source[i] === '\\') i++;
    } else if (c === '/' && source[i + 1] === '/') {
      i = source.indexOf('\n', i);
    } else if (c === '/' && source[i + 1] === '*') {
      i = source.indexOf('*/', i) + 1;
    } else if (c === '{' || c === '[') {
      depth++;
    } else if (c === '}' || c === ']') {
      if (--depth === 0) return source.slice(start, i + 1);
    }
  }
  throw new Error(`${file}: unterminated ${name}`);
}

function evaluate(file, name, scope = []) {
  const source = readFileSync(join(web, file), 'utf8');
  const values = scope.map(other => evaluate(file, other));
  return new Function(...scope, `return (${literal(source, name, file)});`)(...values);
}

function strings(table, where) {
  for (const [key, value] of Object.entries(table)) {
    if (typeof value !== 'string') throw new Error(`${where}.${key} is not a string`);
  }
  return table;
}

const out = Object.fromEntries(LANGS.map(lang => [lang, {}]));
for (const [ns, file, name, shape, order, scope] of TABLES) {
  const table = evaluate(file, name, scope);
  for (const lang of LANGS) out[lang][ns] = {};
  if (shape === 'byLang') {
    for (const lang of LANGS) Object.assign(out[lang][ns], strings(table[lang] ?? {}, `${name}.${lang}`));
    continue;
  }
  for (const [key, value] of Object.entries(table)) {
    for (const lang of LANGS) {
      const text = shape === 'tuple' ? value[order.indexOf(lang)] : value[lang];
      if (text === undefined) continue;
      if (typeof text !== 'string') throw new Error(`${name}.${key}.${lang} is not a string`);
      out[lang][ns][key] = text;
    }
  }
}

// STAT_EXTRA_LABELS has no English column: statNameLabel skipped it for
// en-US and fell through to the detail label, then to the stable ID. Record
// that rendered English so en-US stays the complete key set.
for (const key of Object.keys(out['zh-CN'].statExtra)) {
  out['en-US'].statExtra[key] ??= out['en-US'].detail[key] ?? key;
}
out['en-US'].statExtra = Object.fromEntries(
  Object.keys(out['zh-CN'].statExtra).map(key => [key, out['en-US'].statExtra[key]]),
);

// The language picker listed each language under its own name (Settings.tsx);
// each file now names its language so a new file appears in the picker.
const NAMES = { 'en-US': 'English', 'zh-CN': '简体中文', 'zh-TW': '繁體中文' };
for (const lang of LANGS) out[lang].language = { name: NAMES[lang] };

for (const lang of LANGS) {
  writeFileSync(join(web, 'src/locales', `${lang}.json`), `${JSON.stringify(out[lang], null, 2)}\n`);
  const count = Object.values(out[lang]).reduce((sum, table) => sum + Object.keys(table).length, 0);
  console.log(`${lang}: ${count} strings in ${Object.keys(out[lang]).length} namespaces`);
}
