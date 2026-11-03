// Standalone release demo. No repository files, downloads or npm dependencies.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';

const { values } = parseArgs({ options: {
  package: { type: 'string', default: '.' },
  'build-code': { type: 'string' },
} });
const root = resolve(values.package);
const read = name => readFileSync(join(root, name));
const manifest = JSON.parse(read('manifest.json'));
const engine = await import(pathToFileURL(join(root, 'pobr_wasm.js')).href);
engine.initSync({ module: read('pobr_wasm_bg.wasm') });
assert.equal(engine.schemaVersion(), manifest.schemaVersion, 'WASM / JSON schema mismatch');
const data = JSON.parse(gunzipSync(read('pobr-data.json.gz')).toString('utf8'));
assert.equal(data.version, manifest.dataVersion, 'Game data version mismatch');
assert.equal(Object.keys(data.files).length, manifest.dataFileCount);
for (const [path, content] of Object.entries(data.files)) engine.stageDataFile(path, content);
engine.initStagedData();
assert.ok(engine.isDataReady(), 'Game data initialization failed');
const request = values['build-code']
  ? { pob_code: readFileSync(values['build-code'], 'utf8').trim() }
  : {
    character: { class_name: 'Warrior', level: 90 },
    // Quest rewards default on. Disable the 5% Life reward for an exact +100 probe.
    config_inputs: { 'questInterlude 2Khari CrossingMolten Shrine': false },
  };
const calculate = input => JSON.parse(engine.calculateBuildJson(JSON.stringify(input)));
const lifeOf = result => result.stats.find(stat => stat.id === 'Life')?.value;
const result = calculate(request);
const life = lifeOf(result);
assert.ok(Number.isFinite(life) && life > 0, 'Expected a positive Life result');
assert.ok(Array.isArray(result.unsupported_modifiers));
assert.ok(Array.isArray(result.item_errors));
let lifeWithModifier;
if (!values['build-code']) {
  const modified = calculate({ ...request, extra_modifiers: ['+100 to maximum Life'] });
  lifeWithModifier = lifeOf(modified);
  assert.equal(lifeWithModifier - life, 100, 'Expected +100 maximum Life to add 100 Life');
  assert.equal(modified.unsupported_modifiers.length, 0);
  assert.equal(modified.item_errors.length, 0);
}
console.log(JSON.stringify({ packageVersion: manifest.packageVersion, dataVersion: data.version,
  schemaVersion: engine.schemaVersion(), life, lifeWithModifier,
  unsupportedModifiers: result.unsupported_modifiers.length, itemErrors: result.item_errors.length }, null, 2));
