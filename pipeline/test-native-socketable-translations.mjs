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
