// Opt-in real-WASM measurements. Never included in the ordinary unit-test gate.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve, dirname } from 'node:path';
import { deepStrictEqual } from 'node:assert';

const root = fileURLToPath(new URL('../../', import.meta.url));
const argumentsByName = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  argumentsByName.set(process.argv[index], process.argv[index + 1]);
}
const iterations = Number(argumentsByName.get('--iterations') ?? 9);
if (!Number.isInteger(iterations) || iterations < 3 || iterations > 100) throw new Error('Expected 3..100 iterations');
const currentPath = resolve(argumentsByName.get('--wasm-dir') ?? resolve(root, 'web/src/wasm/pkg'));
const referencePath = argumentsByName.get('--reference');
const outputPath = resolve(argumentsByName.get('--output') ?? resolve(root, '.cache/calc-performance.json'));
const manifest = JSON.parse(await readFile(resolve(root, 'web/public/data/manifest.json'), 'utf8'));
const files = await Promise.all(manifest.files.map(async file => [file,
  await readFile(resolve(root, 'web/public/data', manifest.version, file), 'utf8')]));

async function loadEngine(directory) {
  const bytes = await readFile(resolve(directory, 'pobr_wasm_bg.wasm'));
  const start = performance.now();
  const wasm = await import(pathToFileURL(resolve(directory, 'pobr_wasm.js')).href);
  wasm.initSync({ module: bytes });
  for (const [file, text] of files) wasm.stageDataFile(file, text);
  wasm.initStagedData();
  return { wasm, initializationMs: performance.now() - start,
    wasmSha256: createHash('sha256').update(bytes).digest('hex') };
}

const engines = [];
if (referencePath) engines.push({ name: 'reference', ...await loadEngine(resolve(referencePath)) });
engines.push({ name: 'current', ...await loadEngine(currentPath) });

const statIds = ['TotalDPS', 'TotalEHP', 'Life', 'Mana', 'EnergyShield', 'Armour', 'Evasion',
  'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'ActionRate', 'ManaCost',
  'CritChance', 'ProjectileCount', 'AoeRadius'];
const fixtures = ['monk-invoker-frost-bomb', 'mercenary-tactician-wolf-pack', 'ranger-deadeye-explosive-grenade'];
const measurements = [];
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const normalize = value => Array.isArray(value) ? value.map(normalize) : value && typeof value === 'object'
  ? Object.fromEntries(Object.keys(value).sort().map(key => [key, key === 'unsupported' || key === 'unsupported_modifiers'
    ? [...value[key]].sort() : normalize(value[key])])) : value;

for (const fixture of fixtures) {
  const code = await readFile(resolve(root, 'examples/demo-bd-test/builds', fixture, 'code.txt'), 'utf8');
  const request = { pob_code: code.trim() };
  const variants = Array.from({ length: 16 }, (_, index) => ({
    set_items: [{ slot: 'ring1', text: `Rarity: RARE\nBenchmark Ring\nSapphire Ring\n--------\n+${40 + index} to maximum Life\n${10 + index}% increased Cold Damage` }],
  }));
  for (const scenario of ['single', 'batch-one-stat', 'batch-sixteen-stats']) {
    const method = scenario === 'single' ? 'calculateBuildJson' : 'optimizeVariantsJson';
    const input = scenario === 'single' ? request : {
      request, stats: scenario === 'batch-one-stat' ? ['TotalDPS'] : statIds, variants, include_baseline: true,
    };
    const json = JSON.stringify(input);
    const expected = new Map();
    const samples = new Map(engines.map(engine => [engine.name, []]));
    // Warm each engine separately, then alternate execution order to reduce ordering bias.
    for (const engine of engines) {
      for (let warmup = 1; warmup <= 2; warmup += 1) engine.wasm[method](json + '\n'.repeat(warmup));
    }
    for (let iteration = 0; iteration < iterations; iteration += 1) {
      const order = iteration % 2 === 0 ? engines : [...engines].reverse();
      for (const engine of order) {
        // Distinct, semantically identical JSON bypasses the endpoint response cache.
        // Real planners also send a new request for each batch on the same base build.
        const uncached = json + ' '.repeat(iteration + 1);
        const start = performance.now();
        const response = engine.wasm[method](uncached);
        samples.get(engine.name).push(performance.now() - start);
        const parsed = normalize(JSON.parse(response));
        if (scenario !== 'single') {
          if (parsed.variants.length !== variants.length || parsed.variants.some(row => row.error)) {
            throw new Error(`Invalid variant result: ${fixture}/${scenario}`);
          }
        }
        if (expected.has(engine.name)) deepStrictEqual(parsed, expected.get(engine.name));
        else expected.set(engine.name, parsed);
      }
    }
    if (engines.length === 2) deepStrictEqual(expected.get('current'), expected.get('reference'), `${fixture}/${scenario}`);
    const result = { fixture, scenario, iterations, equivalent: engines.length === 2 ? true : null,
      engines: engines.map(engine => ({ name: engine.name, medianMs: median(samples.get(engine.name)), samplesMs: samples.get(engine.name) })) };
    measurements.push(result);
    console.log(JSON.stringify({ fixture, scenario, equivalent: result.equivalent,
      medians: Object.fromEntries(result.engines.map(engine => [engine.name, Number(engine.medianMs.toFixed(3))])) }));
  }
}

await mkdir(dirname(outputPath), { recursive: true });
await writeFile(outputPath, JSON.stringify({ dataVersion: manifest.version,
  engines: engines.map(({ name, initializationMs, wasmSha256, wasm }) => ({ name, initializationMs, wasmSha256, schema: wasm.schemaVersion() })),
  measurements }, null, 2) + '\n');
