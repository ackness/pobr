import test from 'node:test';
import assert from 'node:assert/strict';
import { nativeSocketableTranslations } from './native-socketable-translations.mjs';

function fixture() {
  const file = 'Data/StatDescriptions/stat_descriptions.csd';
  const pair = { default_reference: { file, record: 2, branch: 0 },
    chinese_reference: { file, record: 2, group: 1, branch: 0 },
    default_template_raw: '+{0} to [Armour]', chinese_template_raw: '+{0} 点[Armour|护甲]',
    declared_chinese_language: 'Simplified Chinese', stat_ids: ['test_armour'],
    restrictions_raw: [], quantifier_tokens_raw: [] };
  const candidate = { format: 'poe2-native-localization-candidate/v1', pairs: [pair],
    input: { manifest_sha256: 'a'.repeat(64), source_identity: { game_patch: 'test' } } };
  const selection = { format: 'pobr-ordinary-facts-receipt/v1',
    adapter: { output_schema: 'pobr_data::catalog::item_overlay::RunesDef' }, target: { version: 'test' },
    conversion: { descriptions: { manifest_sha256: 'a'.repeat(64) }, records: [{
      id: 'rune', mode: 'target-csd', selected_descriptions: [{ source_row: 3, packet: 'normal', prefix: '',
        evidence: [{ pointer: '/files/Data~1StatDescriptions~1stat_descriptions.csd/records/2',
          ids: ['test_armour'], values: [12], branch: 0, handlers: [] }] }] }] } };
  return { candidate, selection, pair };
}

test('uses the selected library branch and values, retaining source references', () => {
  const { candidate, selection } = fixture();
  const result = nativeSocketableTranslations([], selection, candidate);
  assert.deepEqual(result.pairs, [{ en: '+12 to Armour', src: '+12 点护甲' }]);
  assert.equal(result.report.added[0].provenance[0].reference.record, 2);
  selection.conversion.records[0].selected_descriptions[0].evidence[0].values = [37];
  assert.deepEqual(nativeSocketableTranslations([], selection, candidate).pairs,
    [{ en: '+37 to Armour', src: '+37 点护甲' }]);
});

test('never selects a different scope by matching its English text', () => {
  const { candidate, selection, pair } = fixture();
  pair.default_reference.record = 9;
  const result = nativeSocketableTranslations([], selection, candidate);
  assert.deepEqual(result.pairs, []);
  assert.equal(result.report.excluded[0].reason, 'no-library-pair-for-selected-branch');
});

test('scope or language ambiguity is retained, not resolved by first match', () => {
  const { candidate, selection, pair } = fixture();
  candidate.pairs.push({ ...pair, default_reference: { ...pair.default_reference, record: 9 }, chinese_template_raw: '{0} 点护甲上限' });
  const result = nativeSocketableTranslations([], selection, candidate);
  assert.deepEqual(result.pairs, []);
  assert.equal(result.report.excluded[0].reason, 'context-free-translation-would-be-ambiguous');
});

test('existing translation wording and template behavior are preserved', () => {
  const { candidate, selection } = fixture();
  const legacy = [{ en: '+{0} to Armour', src: '护甲 +{0}' }];
  const result = nativeSocketableTranslations(legacy, selection, candidate);
  assert.deepEqual(result.pairs, legacy);
  assert.equal(result.report.excluded[0].reason, 'existing-template-preserved');
});

test('whole socketable compatibility does not acquire target translation claims', () => {
  const { candidate, selection } = fixture();
  selection.conversion.records[0].mode = 'whole-socketable-compatibility';
  assert.deepEqual(nativeSocketableTranslations([], selection, candidate).pairs, []);
});

test('wrong input identity, IDs, handlers and duplicate references are rejected', () => {
  for (const [change, pattern] of [
    [f => { f.candidate.input.manifest_sha256 = 'b'.repeat(64); }, /identity/],
    [f => { f.pair.stat_ids = ['other']; }, /stat IDs/],
    [f => { f.pair.quantifier_tokens_raw = ['negate', '1']; }, /handlers/],
    [f => { f.candidate.pairs.push(f.pair); }, /ambiguous/],
  ]) {
    const f = fixture(); change(f);
    assert.throws(() => nativeSocketableTranslations([], f.selection, f.candidate), pattern);
  }
});

test('signed values use the same selected handler, never an untranslated raw magnitude', () => {
  const { candidate, selection, pair } = fixture();
  pair.quantifier_tokens_raw = ['negate', '1'];
  const evidence = selection.conversion.records[0].selected_descriptions[0].evidence[0];
  evidence.handlers = [{ name_raw: 'negate', argument_tokens_raw: ['1'] }]; evidence.values = [-4];
  assert.deepEqual(nativeSocketableTranslations([], selection, candidate).pairs,
    [{ en: '+4 to Armour', src: '+4 点护甲' }]);
});

test('ordinary tree nodes and ascendancy variants select their own library records', () => {
  const { candidate, selection } = fixture();
  const evidence = selection.conversion.records[0].selected_descriptions[0].evidence;
  selection.adapter.output_schema = 'pobr_data::catalog::{PassiveNodeDef[],PassiveTreeMeta}';
  selection.conversion.projection = { source_records: [{ id: 'base', target_row: 5,
    text: { owner: 'prepared-whole-text' }, variants: [{ class: 'Lich', target: 'variant', source_row: 8,
      text: { owner: 'target-csd', descriptions: evidence } }] }] };
  const result = nativeSocketableTranslations([], selection, candidate);
  assert.deepEqual(result.pairs, [{ en: '+12 to Armour', src: '+12 点护甲' }]);
  assert.equal(result.report.added[0].provenance[0].packet, 'variant:Lich');
  assert.equal(result.report.added[0].provenance[0].id, 'variant');
  selection.conversion.projection.source_records[0].variants[0].text.owner = 'prepared-whole-text';
  assert.deepEqual(nativeSocketableTranslations([], selection, candidate).pairs, []);
});

test('an old prepared tree receipt cannot impersonate selected CSD evidence', () => {
  const { candidate, selection } = fixture();
  selection.adapter.output_schema = 'pobr_data::catalog::{PassiveNodeDef[],PassiveTreeMeta}';
  assert.throws(() => nativeSocketableTranslations([], selection, candidate), /missing target tree projection/);
});

test('uses Rust unit-formatted literals in both languages without a second numeric implementation', () => {
  const { candidate, selection, pair } = fixture();
  const e = selection.conversion.records[0].selected_descriptions[0].evidence[0];
  pair.quantifier_tokens_raw = ['milliseconds_to_seconds_1dp','1','canonical_line'];
  e.handlers = [{name_raw:'milliseconds_to_seconds_1dp',argument_tokens_raw:['1']},
    {name_raw:'canonical_line',argument_tokens_raw:[]}];
  e.values = [2000]; e.formatted_values = ['2.0'];
  assert.deepEqual(nativeSocketableTranslations([], selection, candidate).pairs,
    [{en:'+2.0 to Armour',src:'+2.0 点护甲'}]);
  e.formatted_values = ['1.5']; e.values = [1500];
  assert.equal(nativeSocketableTranslations([], selection, candidate).pairs[0].en,'+1.5 to Armour');
  for (const invalid of [[], ['NaN'], ['Infinity'], ['1,2'], ['{0}'], [' 2'], [2]]) {
    e.formatted_values = invalid;
    assert.throws(() => nativeSocketableTranslations([], selection, candidate), /formatted values/);
  }
});
