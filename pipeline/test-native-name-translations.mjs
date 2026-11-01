import test from 'node:test';
import assert from 'node:assert/strict';
import { nativeNameSidecars } from './native-name-translations.mjs';
const schemas = [['BaseItemTypes', 'Name', 'base_items.json'], ['ActiveSkills', 'DisplayedName', 'skills.json'], ['Mods', 'Name', 'mods.json']];
function fixture() {
  return { candidate: { schema: 'poe2-native-names/v1', status: 'candidate', production_ready: false,
    game: 'poe2', game_patch: '0.5.5.5', realm: 'international', locale: 'zh-TW', language: 'Traditional Chinese',
    entries: schemas.map(([table, field]) => ({ table, field, key_field: 'Id', id: 'current', english: 'Name', localized: '目標', english_row: 0, localized_row: 1 })),
    excluded: schemas.map(([table, field]) => ({ table, field, key_field: 'Id', id: 'empty', reason: 'empty-localized-text', english_row: 1, localized_row: 0 })),
    coverage: Object.fromEntries(schemas.map(([t]) => [t, { paired: 1, english_rows: 2, localized_rows: 2 }])) },
  legacy: Object.fromEntries(schemas.map(([, , f]) => [f, { current: '舊名', empty: '過時', historic: '歷史' }])) };
}
const run = f => nativeNameSidecars(f.candidate, f.legacy, '0.5.5.5', 'zh-TW');
test('target text changes propagate by ID and preserve only explicit historical names', () => {
  const f = fixture(), r = run(f);
  assert.deepEqual(r.files['base_items.json'], { current: '目標', historic: '歷史' });
  assert.equal(r.report.records['base_items.json'].removed[0].legacy_value, '過時');
  f.candidate.entries[0].localized = '新譯';
  assert.equal(run(f).files['base_items.json'].current, '新譯');
});
test('a new source member replaces former historical compatibility; disappearing translation cannot silently retain old value', () => {
  const f = fixture(); f.candidate.excluded[0].id = 'historic';
  const r = run(f); assert.equal(r.files['base_items.json'].historic, undefined);
  assert.equal(r.files['base_items.json'].empty, '過時');
});
test('same English name with different target translations is safe only under distinct IDs', () => {
  const f = fixture(); f.candidate.entries.push({ ...f.candidate.entries[0], id: 'other', localized: '不同', english_row: 2, localized_row: 2 });
  f.candidate.coverage.BaseItemTypes = { paired: 2, english_rows: 3, localized_rows: 3 };
  const r = run(f); assert.equal(r.files['base_items.json'].current, '目標'); assert.equal(r.files['base_items.json'].other, '不同');
});
test('duplicate identities, incomplete inventories and forged unchanged entries reject', () => {
  const f = fixture(); f.candidate.entries.push(f.candidate.entries[0]);
  assert.throws(() => run(f), /duplicate/); f.candidate.entries.pop();
  f.candidate.coverage.BaseItemTypes.english_rows = 3; assert.throws(() => run(f), /Incomplete/);
  f.candidate.coverage.BaseItemTypes.english_rows = 2; f.candidate.entries[0].localized = 'Name'; assert.throws(() => run(f), /paired/);
});
test('Chinese locales and patches cannot be interchanged', () => {
  for (const [key, value] of [['locale', 'zh-CN'], ['language', 'Simplified Chinese'], ['game_patch', '0.5.5.4'], ['realm', 'china']]) {
    const f = fixture(); f.candidate[key] = value; assert.throws(() => run(f), /identity/);
  }
});
