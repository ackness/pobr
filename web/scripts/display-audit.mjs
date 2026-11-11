#!/usr/bin/env node
// What the Web would show for real builds, checked without a browser: the frontend's own engine
// path (WASM exports + the display modules in web/src/lib) run over build codes, reporting
// errors, raw ids in names, untranslated text and unsupported lines.
//
//   node web/scripts/display-audit.mjs [--release <tag> | --package <dir | .tar.gz>]
//        [--refresh] [--langs en-US,zh-CN,zh-TW] [--json FILE] [--samples N] [--no-lines] [--strict]
//        <set dir | build dir | code.txt ...>
//
// Engine: by default the current Web build (web/src/wasm/pkg + web/public/data, from
// `mise run web build`); --package takes a standalone WASM package (a release asset or
// `pnpm --dir web package-wasm` output); --release downloads that release's package once
// (--refresh downloads it again, e.g. after a re-run replaced the asset).
// Builds: a set dir from `mise run builds:fetch` (builds/<name>/code.txt), a dir of
// <name>/code.txt, a build dir or code files. --strict exits 1 on errors or raw-id names.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';

const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
const webDir = join(repoRoot, 'web');
const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    package: { type: 'string' },
    release: { type: 'string' },
    json: { type: 'string' },
    langs: { type: 'string', default: 'en-US,zh-CN,zh-TW' },
    'no-lines': { type: 'boolean', default: false },
    samples: { type: 'string', default: '5' },
    strict: { type: 'boolean', default: false },
    refresh: { type: 'boolean', default: false },
  },
});
if (!positionals.length || (opt.package && opt.release)) {
  console.error('usage: display-audit.mjs [--release <tag> | --package <dir|.tar.gz>] [--json FILE] [--strict] <set dir | build dir | code.txt ...>');
  process.exit(2);
}
const SAMPLES = Number(opt.samples);
if (!Number.isInteger(SAMPLES) || SAMPLES < 0) { console.error(`--samples must be a whole number, got ${opt.samples}`); process.exit(2); }
const LANGS = opt.langs.split(',');

// ---- the Web's display modules, loaded as the browser bundle runs them ----
// Web sources import without extensions ('./chinese'); resolve them like Vite does.
registerHooks({
  resolve(specifier, context, next) {
    try { return next(specifier, context); } catch (error) {
      if (!specifier.startsWith('.') || !context.parentURL?.endsWith('.ts')) throw error;
      for (const suffix of ['.ts', '.tsx', '/index.ts']) {
        try { return next(specifier + suffix, context); } catch {}
      }
      throw error;
    }
  },
  // Vite imports JSON without an attribute; Node requires `type: 'json'`.
  load(url, context, next) {
    if (url.endsWith('.json') && !context.importAttributes?.type)
      return next(url, { ...context, importAttributes: { ...context.importAttributes, type: 'json' } });
    return next(url, context);
  },
});
const webLib = name => import(pathToFileURL(join(webDir, 'src/lib', `${name}.ts`)).href);
const [{ NativeNameIndex, classDisplayName, nativeClassNames, nativeGemNames }, { nativeDisplayTexts }, { skillDisplayName },
  { setGameLocales, lineTemplateLocale }, { GameTextLocalizer }, { isContinuation }] = await Promise.all(
  ['nativeDisplayNames', 'nativeDisplayTexts', 'skillNames', 'gameLocales', 'localizedGameText', 'continuedLines'].map(webLib));

// ---- engine and data ----
function extract(archive) {
  const out = join(repoRoot, '.cache/display-audit', basename(archive).replace(/\.tar\.gz$/, ''));
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  execFileSync('tar', ['-xzf', archive, '-C', out, '--strip-components=1']);
  return out;
}
function releasePackage(tag) {
  const dir = join(repoRoot, '.cache/display-audit', tag);
  const archive = !opt.refresh && existsSync(dir) && readdirSync(dir).find(name => name.endsWith('-web.tar.gz'));
  if (!archive) {
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    execFileSync('gh', ['release', 'download', tag, '--repo', 'ackness/pobr', '--pattern', 'pobr-wasm-*-web.tar.gz',
      '--dir', dir, '--clobber'], { stdio: 'inherit' });
  }
  return extract(join(dir, readdirSync(dir).find(name => name.endsWith('-web.tar.gz'))));
}
function loadEngineSource() {
  const pkg = opt.release ? releasePackage(opt.release)
    : opt.package ? (opt.package.endsWith('.tar.gz') ? extract(resolve(opt.package)) : resolve(opt.package)) : null;
  if (pkg) {
    const data = JSON.parse(gunzipSync(readFileSync(join(pkg, 'pobr-data.json.gz'))).toString('utf8'));
    const manifest = JSON.parse(readFileSync(join(pkg, 'manifest.json'), 'utf8'));
    return { dir: pkg, label: `package ${manifest.packageVersion}`, version: data.version, files: data.files };
  }
  const dir = join(webDir, 'src/wasm/pkg'), dataRoot = join(webDir, 'public/data');
  if (!existsSync(join(dir, 'pobr_wasm.js')) || !existsSync(join(dataRoot, 'manifest.json'))) {
    throw new Error('No Web build found: run `mise run web build`, or pass --package / --release');
  }
  const manifest = JSON.parse(readFileSync(join(dataRoot, 'manifest.json'), 'utf8'));
  if (!Array.isArray(manifest.files)) {
    throw new Error('web/public/data holds a release pack, not per-file data: re-sync it with `mise run web build`, or pass --package');
  }
  // The same bytes the browser fetches under h/<digest>/, read from the plain version directory.
  const files = Object.fromEntries(manifest.files.map(rel => [rel, readFileSync(join(dataRoot, manifest.version, rel), 'utf8')]));
  const version = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')).version;
  return { dir, label: `web build ${version}`, version: manifest.version, files, local: true };
}
const source = loadEngineSource();
const engine = await import(pathToFileURL(join(source.dir, 'pobr_wasm.js')).href);
engine.initSync({ module: readFileSync(join(source.dir, 'pobr_wasm_bg.wasm')) });
// The display modules come from this checkout; they must speak the engine's JSON schema.
const expectedSchema = Number(readFileSync(join(webDir, 'src/api/types.ts'), 'utf8')
  .match(/EXPECTED_SCHEMA_VERSION\s*=\s*(\d+)/)?.[1]);
const engineSchema = typeof engine.schemaVersion === 'function' ? engine.schemaVersion() : undefined;
if (engineSchema !== expectedSchema) {
  const message = `engine schema ${engineSchema ?? 'unknown'} != this checkout's web schema ${expectedSchema}`;
  if (source.local) throw new Error(`${message}: web/src/wasm/pkg is stale, rebuild it with \`mise run web build\``);
  console.error(`warning: ${message}; names come from this checkout's web/src/lib, not that release's`);
}
for (const [path, content] of Object.entries(source.files)) engine.stageDataFile(path, content);
engine.initStagedData();
if (!engine.isDataReady()) throw new Error('game data init failed');
const file = rel => (rel in source.files ? JSON.parse(source.files[rel]) : undefined);
setGameLocales(JSON.parse(engine.availableLocales()));

/** Calls one export; a thrown error or an `{error:…}` JSON body both count as failures. */
function call(name, ...args) {
  try {
    const raw = engine[name](...args);
    const value = typeof raw === 'string' && /^[[{"]/.test(raw) ? JSON.parse(raw) : raw;
    if (value && typeof value === 'object' && !Array.isArray(value) && value.error && Object.keys(value).length <= 2) {
      return { error: typeof value.error === 'string' ? value.error : JSON.stringify(value.error) };
    }
    return { value };
  } catch (error) { return { error: String(error?.message ?? error) }; }
}

// The gem catalog as wasmBackend.gemCatalog builds it.
const catalogs = ['zh-CN', 'zh-TW'].flatMap(locale => {
  const json = file(`i18n/${locale}/display_texts.json`);
  return json ? [nativeDisplayTexts(json, source.version, locale)] : [];
});
const rawGems = JSON.parse(engine.gemCatalogJson());
const gems = catalogs.length && file('base/skill_gems.json')
  ? nativeGemNames(rawGems, file('base/skill_gems.json'), file('overlay/gem_effects.json')?.gems ?? [], catalogs)
  : rawGems;
const byId = new Map(gems.map(entry => [entry.skill_id, entry]));

// The line localizer as wasmBackend.loadDisplayLocalizer + translateLines build it.
const bases = file('base/base_items.json') ?? [];
const cnBases = file('i18n/zh-CN/base_items.json') ?? {}, twBases = file('i18n/zh-TW/base_items.json') ?? {};
const localizer = new GameTextLocalizer([
  ...Object.entries(file('i18n/zh-CN/words.json') ?? {}).map(([name, name_zh_cn]) => ({ name, name_zh_cn })),
  ...bases.map(base => ({ name: base.name, name_zh_cn: cnBases[base.id], name_zh_tw: twBases[base.id] })),
  ...rawGems,
], catalogs);
// Each text with its origin: null for official text, 'agent' / 'human' for the translation
// repository's own supplement. A package without translateLinesDetailed has only official text.
function translate(lines, lang) {
  const official = text => ({ text, origin: null });
  if (lang === 'en-US' || !lines.length) return lines.map(official);
  const templates = lineTemplateLocale(lang);
  const rendered = !templates ? lines.map(official)
    : engine.translateLinesDetailed ? JSON.parse(engine.translateLinesDetailed(templates, JSON.stringify(lines)))
      : JSON.parse(engine.translateLines(templates, JSON.stringify(lines))).map(official);
  return localizer.localizeLinesDetailed(lines, rendered, lang);
}
// Names of known identity as wasmBackend.translateNames shows them: passive node names by
// PassiveSkills id, classified base lines by base identity, the rest through translate().
const nameIndex = new NativeNameIndex(catalogs, bases);
function translateEntries(entries, lang) {
  const named = entries.filter(entry => entry.name).map(entry => entry.name);
  const { pending, mergeDetailed } = nameIndex.split(named, lang);
  const names = lang === 'en-US' ? named.map(name => ({ text: name.name, origin: null })) : mergeDetailed(translate(pending, lang));
  const texts = translate(entries.filter(entry => !entry.name).map(entry => entry.text), lang);
  let n = 0, t = 0;
  return entries.map(entry => entry.name ? names[n++] : texts[t++]);
}
// Class labels as wasmBackend.loadClassNames + classDisplayName build them.
const treeMeta = file('base/passive_tree_meta.json');
const classNames = catalogs.length && treeMeta
  ? nativeClassNames(file('i18n/zh-CN/classes.json') ?? { classes: {}, ascendancies: {} }, treeMeta, catalogs)
  : file('i18n/zh-CN/classes.json') ?? { classes: {}, ascendancies: {} };
const tree = new Map((file('base/passive_tree.json') ?? []).map(node => [node.skill, node]));

// ---- checks ----
const CJK = /[㐀-鿿]/;
const PLACEHOLDER = /\?\?\?|coming soon|敬请期待|敬請期待|\{\d\}/i;
const LEAKED_VALUE = /\bundefined\b|\bnull\b|\bNaN\b|\[object Object\]/;
function rawIdReason(text, skillId) {
  if (!text?.trim()) return 'empty';
  if (/^Metadata\//.test(text)) return 'metadata id';
  if (text === skillId) return 'skill id';
  if (/^[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+){2,}$/.test(text)) return 'CamelCase id';
  if (/^[a-z0-9]+(?:_[a-z0-9+%]+)+$/.test(text)) return 'snake_case key';
  if (PLACEHOLDER.test(text) || LEAKED_VALUE.test(text)) return 'placeholder';
  return null;
}
/** Text that should be Chinese but is not: Latin words and no CJK at all. */
const untranslated = text => !CJK.test(text) && /[A-Za-z]{3,}/.test(text.replace(/\{[^}]*\}|[+-]?\d[\d.,%]*/g, ''));
const statOf = (stats, id) => stats.find(stat => stat.id === id)?.value;

function auditBuild(name, code) {
  const r = { name, problems: [], groups: [], counts: {} };
  const add = (kind, detail) => r.problems.push({ kind, detail });

  const decoded = call('decodeBuildJson', code);
  if (decoded.error) { add('decode error', decoded.error); return r; }
  const build = decoded.value;
  r.character = `${build.character?.class_name}/${build.character?.ascendancy_name} L${build.character?.level}`;
  for (const [kind, english] of [['classes', build.character?.class_name], ['ascendancies', build.character?.ascendancy_name]]) {
    if (!english) continue;
    for (const lang of LANGS.filter(l => l !== 'en-US')) {
      const shown = classDisplayName(classNames, english, lang, kind);
      if (untranslated(shown)) add(`${kind === 'classes' ? 'class' : 'ascendancy'} name untranslated [${lang}]`, `${english} → "${shown}"`);
    }
  }

  // Socket groups as the Skills page labels them.
  const label = (gem, lang) => gem.skill_id ? skillDisplayName(gem.skill_id, lang, byId)
    : gem.name_spec ? `${gem.name_spec} · (unresolved)` : '(unresolved)';
  build.socket_groups.forEach((group, index) => {
    const row = { index, slot: group.slot, source: group.source, enabled: group.enabled, gems: [] };
    for (const gem of group.gems) {
      const names = Object.fromEntries(LANGS.map(lang => [lang, label(gem, lang)]));
      row.gems.push({ skill_id: gem.skill_id, ...names });
      if (!gem.skill_id) { add('unresolved gem', `g${index}: ${gem.name_spec ?? '(no name)'}`); continue; }
      if (!byId.has(gem.skill_id)) {
        const isSecondary = [...byId.values()].some(e => e.additional_skill_ids?.includes(gem.skill_id));
        add(isSecondary ? 'gem shown via secondary-skill alias' : 'gem not in catalog (prettified id)',
          `g${index}: ${gem.skill_id} → "${names[LANGS[0]]}"`);
      }
      for (const lang of LANGS) {
        const why = rawIdReason(names[lang], gem.skill_id);
        if (why) add(`gem name is ${why} [${lang}]`, `g${index}: ${gem.skill_id} → "${names[lang]}"`);
        else if (lang !== 'en-US' && untranslated(names[lang])) add(`gem name untranslated [${lang}]`, `g${index}: ${gem.skill_id} → "${names[lang]}"`);
      }
    }
    r.groups.push(row);
  });

  // The calculation the session requests after import (full DPS when any group asks for it).
  const request = { pob_code: code, breakdowns: [],
    ...(build.socket_groups.some(group => group.include_in_full_dps) ? { full_dps: true } : {}) };
  const calc = call('calculateBuildJson', JSON.stringify(request));
  if (calc.error) { add('calculate error', calc.error); return r; }
  const out = calc.value;
  const stats = out.stats ?? [];
  const life = statOf(stats, 'Life'), es = statOf(stats, 'EnergyShield');
  r.summary = { life, es, combined_dps: out.main_skill?.combined_dps, full_dps: out.full_dps?.dps,
    main_skill: out.main_skill?.skill_id ?? null, minions: (out.minions ?? []).map(m => `${m.name}:${Math.round(m.dps)}`) };
  if (!out.main_skill) add('main skill unresolved', `main_socket_group=${build.main_socket_group}`);
  else {
    for (const lang of LANGS) {
      const shown = skillDisplayName(out.main_skill.skill_id, lang, byId);
      const why = rawIdReason(shown, out.main_skill.skill_id);
      if (why) add(`main skill name is ${why} [${lang}]`, `${out.main_skill.skill_id} → "${shown}"`);
    }
    const minionDps = (out.minions ?? []).reduce((sum, m) => sum + (m.dps || 0), 0);
    const trigger = out.unmodelled_meta_trigger;
    if (trigger) {
      const names = LANGS.map(lang => `${lang} "${skillDisplayName(trigger.meta_gem_id, lang, byId)}"`).join(', ');
      add('0 DPS explained: unmodelled meta trigger', `${trigger.host_skill_id} ← ${trigger.meta_gem_id} (${names})`);
    } else if (!(out.main_skill.combined_dps > 0) && !(minionDps > 0) && !(out.full_dps?.dps > 0)) {
      add('0 DPS (main, minions and full)', `${out.main_skill.skill_id} combined=${out.main_skill.combined_dps}`);
    }
  }
  for (const stat of stats) {
    if (typeof stat.value === 'number' && !Number.isFinite(stat.value)) add('non-finite stat', `${stat.id}=${stat.value}`);
  }
  if (!(life > 0) && !(es > 0)) add('Life and ES <= 0', `Life=${life} ES=${es}`);
  for (const minion of out.minions ?? []) {
    const why = rawIdReason(minion.name, minion.minion_id);
    if (why) add(`minion name is ${why}`, `${minion.minion_id} → "${minion.name}"`);
    if (!(minion.life > 0)) add('minion Life <= 0', `${minion.name} life=${minion.life}`);
    // Calcs shows the MonsterVarieties.Name record of the minion's variety, else minion.name.
    const shown = LANGS.filter(lang => lang !== 'en-US').map(lang => [lang, nameIndex.resolveDetailed(
      { kind: 'monster', id: minion.monster_variety_id, name: minion.name ?? '' }, lang) ?? { text: minion.name ?? '', origin: null }]);
    if (shown.some(([, name]) => untranslated(name.text))) {
      add('minion name shown in English', `${minion.minion_id} → "${minion.name}"`);
    }
    for (const [lang, name] of shown) {
      if (name.origin && !untranslated(name.text)) add(`minion name is supplement text [${lang}]`, `${minion.minion_id} → "${name.text}" (${name.origin})`);
    }
  }
  for (const skill of out.full_dps?.skills ?? []) {
    if (skill.error) add('full DPS skill error', `${skill.skill_id}: ${skill.error}`);
  }
  r.counts.unsupported = out.unsupported_modifiers?.length ?? 0;
  for (const line of out.unsupported_modifiers ?? []) add('unsupported modifier', line);
  for (const issue of out.item_errors ?? []) add('item error', JSON.stringify(issue));
  for (const item of build.items?.equipped ?? []) if (!item.text?.trim()) add('missing item (empty text)', item.slot);

  // Other exports the UI calls with the same request.
  const usage = call('lineUsageJson', JSON.stringify(request));
  if (usage.error) add('lineUsage error', usage.error);
  else {
    for (const u of usage.value.usage?.unparsed ?? []) add('unparsed line', u.text);
    for (const g of usage.value.usage?.gem_stats ?? []) add(`gem stat ${g.status}`, `${g.owner}: ${g.stat}${g.reason ? ` (${g.reason})` : ''}`);
  }
  const metrics = call('skillMetricsJson', JSON.stringify(request));
  if (metrics.error) add('skillMetrics error', metrics.error);

  // Item and passive lines through translateLines + the localizer, batched as the Web sends them:
  // one item's lines together (a line the display does not translate, or translates locally, is
  // sent blank and separates its neighbours), one node's lines together. Adjacent lines of one
  // multi-line description translate as one, never across items or nodes.
  if (!opt['no-lines']) {
    const itemGroups = [];
    for (const item of [...(build.items?.equipped ?? []), ...(build.items?.flasks ?? []), ...(build.items?.limbs ?? []),
      ...(build.items?.socket_jewels ?? []), ...(build.items?.equipment_jewels ?? [])]) {
      const classified = call('classifyItemLinesJson', item.text);
      if (classified.error) { add('classifyItemLines error', `${item.slot ?? item.socket_node}: ${classified.error}`); continue; }
      itemGroups.push(classified.value.map(line =>
        ['implicit', 'explicit', 'enchant', 'rune', 'name', 'base'].includes(line.kind) && line.text.trim()
          ? { where: item.slot ?? `socket ${item.socket_node}`, kind: line.kind, text: line.text,
            ...(line.kind === 'base' ? { name: { kind: 'base', name: line.text } } : {}) }
          : { separator: true, text: '' }));
    }
    const nodeGroups = [];
    for (const id of build.tree?.allocated_nodes ?? []) {
      const node = tree.get(id);
      if (!node) { add('allocated node not in tree', String(id)); continue; }
      const where = `node ${id} ${node.id} ${node.name ?? ''}`;
      nodeGroups.push([node.name ? { where, kind: 'passive name', text: node.name, name: { kind: 'passive', id: node.id, name: node.name } }
        : { where, kind: 'passive name', text: node.id },
      ...(node.stats ?? []).map(text => ({ where, kind: 'passive',
        text: text.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1') }))]);
    }
    for (const lang of LANGS.filter(l => l !== 'en-US')) {
      for (const [kind, groups] of [['item', itemGroups], ['passive', nodeGroups]]) {
        let rendered;
        try { rendered = groups.flatMap(group => translateEntries(group, lang)); } catch (error) { add(`translateLines error [${lang}]`, String(error)); continue; }
        const lines = groups.flat();
        let missed = 0, counted = 0, supplement = 0;
        rendered.forEach(({ text, origin }, i) => {
          if (lines[i].separator) return;
          counted++;
          // A line a multi-line description translated into the line above shows no row.
          if (isContinuation(lines[i].text, text)) return;
          const why = rawIdReason(text);
          if (why && why !== 'empty') add(`${kind} line shows ${why} [${lang}]`, `${lines[i].where}: "${text}"`);
          else {
            const what = kind === 'item' ? `item ${lines[i].kind} line` : lines[i].kind === 'passive name' ? 'passive name' : 'passive line';
            if (untranslated(text)) {
              missed++;
              add(`${what} untranslated [${lang}]`, `${lines[i].where}: ${text}`);
            } else if (origin) {
              // Translated, but by the translation repository rather than the official client.
              supplement++;
              add(`${what} is supplement text [${lang}]`, `${lines[i].where}: ${text} (${origin})`);
            }
          }
        });
        // `translated/counted`; the supplement share of the translated lines only when there is one.
        r.counts[`${kind}_lines_${lang}`] = `${counted - missed}/${counted}`;
        if (supplement) r.counts[`${kind}_supplement_lines_${lang}`] = `${supplement}/${counted - missed}`;
      }
    }
  }
  return r;
}

/** Report sections, most serious first. `strict` sections fail --strict. */
const SECTIONS = [
  { title: 'Errors', strict: true, test: kind => /error|exception|unresolved$|non-finite|<= 0|missing item/.test(kind) },
  { title: 'Raw ids and placeholders in names', strict: true, test: kind => / is (metadata id|skill id|CamelCase id|snake_case key|placeholder|empty)| shows /.test(kind) },
  { title: 'Names and lines left untranslated', test: kind => /untranslated|in English/.test(kind) },
  // Translated lines and names whose text is the translation repository's own, not the client's.
  { title: 'Supplement (non-official) text', test: kind => / is supplement text /.test(kind) },
  { title: 'Calculation coverage', test: kind => /^unsupported|^unparsed|^gem stat|not in tree|^0 DPS/.test(kind) },
  // Unresolved gems ("· (unresolved)" on the Skills page) and catalog fallbacks.
  { title: 'Other', test: () => true },
];
const sectionOf = kind => SECTIONS.find(section => section.test(kind));

// ---- inputs ----
function codeFiles(path) {
  const abs = resolve(path);
  if (statSync(abs).isFile()) return [abs];
  const dir = existsSync(join(abs, 'builds')) ? join(abs, 'builds') : abs;
  if (existsSync(join(dir, 'code.txt'))) return [join(dir, 'code.txt')];
  return readdirSync(dir).sort().map(entry => join(dir, entry, 'code.txt')).filter(existsSync);
}
const inputs = positionals.flatMap(codeFiles);
if (!inputs.length) { console.error('no code.txt found in the given paths'); process.exit(2); }
const results = [];
for (const path of inputs) {
  const name = basename(path) === 'code.txt' ? basename(dirname(path)) : basename(path);
  const started = Date.now();
  let result;
  try { result = auditBuild(name, readFileSync(path, 'utf8').trim()); }
  catch (error) { result = { name, problems: [{ kind: 'audit exception', detail: String(error?.stack ?? error) }], groups: [] }; }
  result.ms = Date.now() - started;
  results.push(result);
  process.stderr.write(`${name}: ${result.problems.length} findings (${result.ms} ms)\n`);
}

// ---- report ----
const tally = new Map();
for (const r of results) {
  for (const p of r.problems) {
    const t = tally.get(p.kind) ?? { builds: new Set(), total: 0, examples: [] };
    t.builds.add(r.name); t.total++;
    if (t.examples.length < SAMPLES && !t.examples.includes(p.detail)) t.examples.push(p.detail);
    tally.set(p.kind, t);
  }
}
// Raw ids only: the catalog also lists internal placeholder skills ("Coming Soon").
const isRawId = (text, id) => !['placeholder', null].includes(rawIdReason(text, id));
const catalogRaw = Object.fromEntries(LANGS.map(lang =>
  [lang, gems.filter(g => isRawId(skillDisplayName(g.skill_id, lang, byId), g.skill_id)).map(g => g.skill_id)]));
console.log(`# PoBR display audit — ${source.label}, data ${source.version}, ${results.length} builds\n`);
console.log(`Gem catalog: ${gems.length} skills; names that are raw ids: `
  + LANGS.map(lang => `${lang} ${catalogRaw[lang].length}`).join(', '));
const sample = [...new Set(Object.values(catalogRaw).flat())].slice(0, SAMPLES);
if (sample.length) console.log(`  e.g. ${sample.map(id => `${id} → "${skillDisplayName(id, catalogRaw[LANGS[0]].includes(id) ? LANGS[0] : LANGS.at(-1), byId)}"`).join(', ')}`);
let strictFailures = 0;
for (const section of SECTIONS) {
  const rows = [...tally].filter(([kind]) => sectionOf(kind) === section)
    .sort((a, b) => b[1].builds.size - a[1].builds.size || b[1].total - a[1].total);
  if (!rows.length) continue;
  if (section.strict) strictFailures += rows.length;
  console.log(`\n## ${section.title}\n`);
  for (const [kind, t] of rows) {
    console.log(`- **${kind}**: ${t.builds.size}/${results.length} builds, ${t.total} times`);
    for (const example of t.examples) console.log(`    - ${example.slice(0, 200)}`);
  }
}
console.log('\n## Per build\n');
for (const r of results) {
  const kinds = {};
  for (const p of r.problems) kinds[p.kind] = (kinds[p.kind] ?? 0) + 1;
  const s = r.summary ?? {};
  console.log(`- ${r.name} (${r.character ?? '?'}): main ${s.main_skill ? skillDisplayName(s.main_skill, 'en-US', byId) : '-'}, `
    + `Life ${s.life} ES ${s.es}, DPS ${s.combined_dps?.toFixed?.(0) ?? '-'} (full ${s.full_dps?.toFixed?.(0) ?? '-'}), `
    + `${r.problems.length} findings${Object.keys(kinds).length ? ': ' : ''}`
    + Object.entries(kinds).sort((a, b) => b[1] - a[1]).slice(0, 4).map(([k, n]) => `${k} ×${n}`).join(', '));
}
if (opt.json) writeFileSync(opt.json, JSON.stringify({ source: source.label, data: source.version, catalogRaw, results }, null, 2));
if (opt.strict && (strictFailures || Object.values(catalogRaw).some(ids => ids.length))) process.exit(1);
