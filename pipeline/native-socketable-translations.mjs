// Select literal display translations by the actual CSD record/branch consumed
// by ordinary socketable/tree producers. Never flatten the complete native archive.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const plain = text => text.replace(/\[([^\]|]+)(?:\|([^\]]+))?\]/gu, (_, id, label) => label ?? id)
  .replace(/\\n/gu, '\n');
const skeleton = text => text.replace(/\{[^{}]*\}/gu, '').replace(/[0-9+\-.,\s]/gu, '').toLowerCase();
const key = ref => JSON.stringify([ref.file, ref.record, ref.branch]);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const required = (ok, message) => { if (!ok) throw new Error(message); };

function format(template, rawValues, tokens, formatted) {
  const values = [...rawValues];
  required(values.every(Number.isSafeInteger), 'unsafe source integer');
  if (formatted !== undefined) {
    // Unit conversion belongs to the pinned Rust renderer. A verified receipt
    // supplies its decimal literals; both languages use precisely those values.
    required(Array.isArray(formatted) && formatted.length === values.length
      && formatted.every(v => typeof v === 'string' && /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:e[+-]\d+)?$/u.test(v)),
    'invalid Rust formatted values');
    return substitute(template, formatted);
  }
  required(tokens.length % 2 === 0, 'unsupported translation handler arity');
  for (let i = 0; i < tokens.length; i += 2) {
    const index = Number(tokens[i + 1]) - 1;
    required(Number.isInteger(index) && index >= 0 && index < values.length, 'handler index outside packet');
    const scale = { negate: -1, double: 2, negate_and_double: -2 }[tokens[i]];
    required(scale !== undefined, 'unsupported translation handler');
    values[index] *= scale;
    required(Number.isSafeInteger(values[index]), 'translation integer overflow');
  }
  return substitute(template, values.map(String));
}

function substitute(template, values) {
  // Same source numbering contract as passive_tree::render::format_template:
  // an explicit index resets the next implicit slot; escapes are literal and
  // substituted values are never reparsed. Numeric conversion stays in Rust.
  let rendered = '', nextIndex = 0;
  for (let cursor = 0; cursor < template.length;) {
    const escape = template.slice(cursor, cursor + 2);
    if (escape === '{{' || escape === '}}') {
      rendered += escape[0]; cursor += 2; continue;
    }
    const ch = template[cursor++];
    required(ch !== '}', 'unmatched translation brace');
    if (ch !== '{') { rendered += ch; continue; }
    const end = template.indexOf('}', cursor);
    required(end !== -1, 'unclosed translation placeholder');
    const match = /^(\d*)(?::(d|\+d|-d)?)?$/u.exec(template.slice(cursor, end));
    required(match, 'unsupported translation placeholder');
    const index = match[1] === '' ? nextIndex : Number(match[1]);
    required(Number.isSafeInteger(index) && index < values.length, 'translation placeholder outside packet');
    nextIndex = index + 1;
    const value = values[index];
    rendered += match[2] === '+d' && !value.startsWith('-') ? `+${value}` : value;
    cursor = end + 1;
  }
  return plain(rendered.replaceAll('%%', '%'));
}

/** Preserve existing wording and add only context-bound, globally unambiguous literals. */
export function nativeSelectedTranslations(legacy, selection, candidate) {
  required(Array.isArray(legacy) && legacy.every(p => typeof p.src === 'string' && typeof p.en === 'string'), 'invalid legacy translation pairs');
  required(selection.format === 'pobr-ordinary-facts-receipt/v1', 'not an ordinary producer receipt');
  const schema = selection.adapter?.output_schema;
  let records;
  const unselectedFields = [];
  if (schema === 'pobr_data::catalog::item_overlay::RunesDef') records = selection.conversion.records;
  else if (schema === 'pobr_data::catalog::{PassiveNodeDef[],PassiveTreeMeta}') {
    records = [];
    required(Array.isArray(selection.conversion?.projection?.source_records), 'missing target tree projection');
    for (const node of selection.conversion.projection.source_records) {
      const packets = [{ id: node.id, row: node.target_row, packet: 'node', text: node.text },
        ...node.variants.map(v => ({ id: v.target, row: v.source_row, packet: `variant:${v.class}`, text: v.text }))];
      for (const p of packets) {
        if (!['target-csd', 'target-csd-and-client-strings'].includes(p.text?.owner)) continue;
        if (p.text.owner === 'target-csd-and-client-strings') {
          required(Array.isArray(p.text.non_stat), 'missing source ClientStrings evidence');
          // This CSD library has no ClientStrings/name bindings. Preserve their
          // separate ownership and report them instead of matching English.
          for (const field of p.text.non_stat) {
            unselectedFields.push({ reason: 'client-string-requires-separate-language-binding',
              provenance: { id: p.id, packet: p.packet, source_row: p.row }, evidence: field });
          }
        }
        records.push({ id: p.id, mode: 'target-csd', selected_descriptions: [{
          packet: p.packet, source_row: p.row, prefix: '', evidence: p.text.descriptions }] });
      }
    }
  } else throw new Error('unsupported ordinary selection schema');
  required(candidate.format === 'poe2-native-localization-candidate/v1' && Array.isArray(candidate.pairs), 'invalid native translation export');
  required(candidate.input?.manifest_sha256 === selection.conversion?.descriptions?.manifest_sha256
    && candidate.input?.source_identity?.game_patch === selection.target?.version, 'translation/selected fact identity mismatch');
  const index = new Map(), english = new Map(), chinese = new Map();
  for (const pair of candidate.pairs) {
    const k = key(pair.default_reference);
    required(!index.has(k), 'ambiguous native branch translation');
    index.set(k, pair);
    const en = plain(pair.default_template_raw), src = plain(pair.chinese_template_raw);
    // Different contexts are retained by the language library. A context-free
    // ordinary text lookup cannot represent any disagreement in this bucket.
    const signature = JSON.stringify([en.toLowerCase(), src, pair.quantifier_tokens_raw, pair.restrictions_raw]);
    for (const [map, text] of [[english, en], [chinese, src]]) {
      const bucket = skeleton(text);
      if (!map.has(bucket)) map.set(bucket, new Set());
      map.get(bucket).add(signature);
    }
  }
  const proposals = [], excluded = [...unselectedFields];
  for (const record of records) {
    if (record.mode !== 'target-csd') continue;
    for (const packet of record.selected_descriptions) {
      for (const evidence of packet.evidence) {
        const match = /^\/files\/(.+)\/records\/(\d+)$/u.exec(evidence.pointer);
        required(match, 'invalid selected description pointer');
        const ref = { file: match[1].replace(/~1/gu, '/').replace(/~0/gu, '~'), record: Number(match[2]), branch: evidence.branch };
        const pair = index.get(key(ref));
        const provenance = { id: record.id, packet: packet.packet, source_row: packet.source_row, reference: ref, values: evidence.values, formatted_values: evidence.formatted_values, equivalent_english_descriptors: evidence.equivalent_english_descriptors };
        if (!pair) { excluded.push({ reason: 'no-library-pair-for-selected-branch', provenance }); continue; }
        required(pair.declared_chinese_language === 'Simplified Chinese'
          && /\p{Script=Han}/u.test(pair.chinese_template_raw), 'invalid selected Chinese wording');
        required(same(pair.stat_ids, evidence.ids), 'selected stat IDs differ from language pair');
        const handlers = evidence.handlers.flatMap(q => [q.name_raw, ...q.argument_tokens_raw]);
        required(same(pair.quantifier_tokens_raw, handlers) && same(pair.restrictions_raw, []), 'selected language scope/handlers differ');
        if (english.get(skeleton(plain(pair.default_template_raw))).size !== 1
            || chinese.get(skeleton(plain(pair.chinese_template_raw))).size !== 1) {
          excluded.push({ reason: 'context-free-translation-would-be-ambiguous', provenance }); continue;
        }
        const en = format(pair.default_template_raw, evidence.values, handlers, evidence.formatted_values).split('\n');
        const src = format(pair.chinese_template_raw, evidence.values, handlers, evidence.formatted_values).split('\n');
        if (en.length !== src.length) { excluded.push({ reason: 'multiline-alignment-unproven', provenance }); continue; }
        // Bonded is an existing item syntax marker, not a newly authored translation.
        // Keep it intact; this producer owns only the library's effect wording.
        for (let i = 0; i < en.length; i++) {
          proposals.push({ en: packet.prefix + en[i], src: packet.prefix + src[i], provenance: {
            ...provenance, chinese_reference: pair.chinese_reference, stat_ids: pair.stat_ids, line: i } });
        }
      }
    }
  }
  const pairs = [...legacy], existingEn = new Set(legacy.map(p => skeleton(p.en))), existingCn = new Set(legacy.map(p => skeleton(p.src)));
  const unique = new Map(), enBuckets = new Map(), cnBuckets = new Map();
  for (const p of proposals) {
    const k = JSON.stringify([p.en, p.src]);
    if (!unique.has(k)) unique.set(k, { en: p.en, src: p.src, provenance: [] });
    unique.get(k).provenance.push(p.provenance);
    for (const [map, text, opposite] of [[enBuckets, p.en, p.src], [cnBuckets, p.src, p.en]]) {
      if (!map.has(text)) map.set(text, new Set());
      map.get(text).add(opposite);
    }
  }
  const added = [];
  for (const p of unique.values()) {
    const reason = existingEn.has(skeleton(p.en)) || existingCn.has(skeleton(p.src)) ? 'existing-template-preserved'
      : enBuckets.get(p.en).size !== 1 || cnBuckets.get(p.src).size !== 1 ? 'literal-translation-conflict' : null;
    if (reason) excluded.push({ reason, ...p });
    else { pairs.push({ src: p.src, en: p.en }); added.push(p); }
  }
  pairs.sort((a, b) => a.src < b.src ? -1 : a.src > b.src ? 1 : a.en < b.en ? -1 : a.en > b.en ? 1 : 0);
  return { pairs, report: { added, excluded, legacy_pairs: legacy.length,
    library_input: candidate.input, library_consumer: candidate.consumer,
    scope: 'Selected, unambiguous literal effect text only. Existing templates and untranslatable effects remain intact; no name translation or CN numeric equivalence claim.' } };
}

// Keep the original import API for existing socketable callers.
export const nativeSocketableTranslations = nativeSelectedTranslations;

function pinned(file, expected) {
  required(/^[a-f0-9]{64}$/u.test(expected), 'invalid external SHA256');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NONBLOCK | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    required(stat.isFile() && stat.size <= 128 * 1024 * 1024, 'translation input is not a bounded regular file');
    const bytes = fs.readFileSync(fd);
    required(digest(bytes) === expected, 'translation input SHA256 mismatch');
    return JSON.parse(bytes.toString('utf8'));
  } finally { fs.closeSync(fd); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = {};
  const flags = ['selection', 'selection-sha256', 'native', 'native-sha256', 'legacy', 'legacy-sha256', 'out'];
  for (let i = 2; i < process.argv.length; i += 2) {
    const flag = process.argv[i].replace(/^--/u, '');
    required(flags.includes(flag) && !Object.hasOwn(options, flag) && process.argv[i + 1], 'invalid or duplicate translation option');
    options[flag] = process.argv[i + 1];
  }
  required(flags.every(f => options[f]), 'missing translation argument');
  const result = nativeSocketableTranslations(pinned(options.legacy, options['legacy-sha256']),
    pinned(options.selection, options['selection-sha256']), pinned(options.native, options['native-sha256']));
  const out = path.resolve(options.out);
  // A fresh candidate directory prevents in-place edits to the language repository or runtime snapshot.
  fs.mkdirSync(out);
  const lines = JSON.stringify(result.pairs, null, 2) + '\n';
  fs.writeFileSync(path.join(out, 'stat_lines.json'), lines, { flag: 'wx' });
  fs.writeFileSync(path.join(out, 'receipt.json'), JSON.stringify({ ...result.report, inputs: options,
    files: { 'stat_lines.json': { bytes: Buffer.byteLength(lines), sha256: digest(lines) } },
    implementation_sha256: digest(fs.readFileSync(fileURLToPath(import.meta.url))), adoption: 'not-adopted' }, null, 2) + '\n', { flag: 'wx' });
  console.log(`native selected translations: ${result.report.added.length} literal pairs added; ${result.report.excluded.length} selections retained/excluded`);
}
