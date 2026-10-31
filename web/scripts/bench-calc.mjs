// Opt-in real-WASM measurements. Never included in the ordinary unit-test gate.
import { readFile, writeFile, mkdir, lstat, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { platform, arch, release, cpus } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve, dirname } from 'node:path';
import { deepStrictEqual } from 'node:assert';
import { bindingFiles } from './wasm-build-inputs.mjs';

const started = performance.now();
const root = fileURLToPath(new URL('../../', import.meta.url));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const argumentsByName = new Map();
const flags = new Set(['--iterations', '--wasm-dir', '--reference', '--output']);
for (let index = 2; index < process.argv.length; index += 2) {
  const flag = process.argv[index];
  const value = process.argv[index + 1];
  if (!flags.has(flag) || argumentsByName.has(flag)) throw new Error(`Unknown or duplicate option: ${flag}`);
  if (!value?.trim() || value.startsWith('--')) throw new Error(`Missing value for ${flag}`);
  argumentsByName.set(flag, value);
}
const iterations = Number(argumentsByName.get('--iterations') ?? 9);
if (!Number.isInteger(iterations) || iterations < 3 || iterations > 100) throw new Error('Expected 3..100 iterations');
const currentPath = resolve(argumentsByName.get('--wasm-dir') ?? resolve(root, 'web/src/wasm/pkg'));
const referencePath = argumentsByName.get('--reference');
if (referencePath && await realpath(referencePath) === await realpath(currentPath)) {
  throw new Error('Reference and current engines must use separate binding directories');
}
const outputPath = resolve(argumentsByName.get('--output') ?? resolve(root, '.cache/calc-performance.json'));
try {
  await lstat(outputPath);
  throw new Error(`Refusing to overwrite an observation: ${outputPath}`);
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
const dataReadStarted = performance.now();
const manifestBytes = await readFile(resolve(root, 'web/public/data/manifest.json'));
const manifest = JSON.parse(manifestBytes.toString('utf8'));
if (!Array.isArray(manifest.files) || !manifest.files.length || new Set(manifest.files).size !== manifest.files.length
    || typeof manifest.version !== 'string' || !/^[a-zA-Z0-9._-]+$/.test(manifest.version)
    || manifest.version === '.' || manifest.version === '..') throw new Error('Invalid benchmark data manifest');
const files = await Promise.all(manifest.files.map(async file => [file,
  await readFile(dataPath(file), 'utf8')]));
const dataReadMs = performance.now() - dataReadStarted;
const dataVerificationStarted = performance.now();
const dataInventory = files.map(([file, text]) => {
  const digest = sha256(text);
  if (digest !== manifest.digests?.[file]) throw new Error(`Data manifest hash mismatch: ${file}`);
  return { file, bytes: Buffer.byteLength(text), sha256: digest };
});
const dataVerificationMs = performance.now() - dataVerificationStarted;

function dataPath(file) {
  if (typeof file !== 'string' || file.includes('\\') || file.split('/').some(part => !part || part === '.' || part === '..')) {
    throw new Error(`Unsafe benchmark data path: ${file}`);
  }
  const versionRoot = resolve(root, 'web/public/data', manifest.version);
  const path = resolve(versionRoot, file);
  if (!path.startsWith(versionRoot + '/')) throw new Error(`Unsafe benchmark data path: ${file}`);
  return path;
}

async function loadEngine(directory) {
  const readStarted = performance.now();
  const artifacts = new Map(await Promise.all(bindingFiles.map(async file => [file, await readFile(resolve(directory, file))])));
  const receiptBytes = await readFile(resolve(directory, 'build-receipt.json'));
  const artifactReadMs = performance.now() - readStarted;
  const verificationStarted = performance.now();
  const receipt = JSON.parse(receiptBytes.toString('utf8'));
  if (receipt.version !== 1 || !Number.isInteger(receipt.schemaVersion) || receipt.schemaVersion < 1
      || bindingFiles.some(file => receipt.files?.[file] !== sha256(artifacts.get(file)))) {
    throw new Error(`WASM build receipt mismatch: ${directory}`);
  }
  const artifactVerificationMs = performance.now() - verificationStarted;
  const bytes = artifacts.get('pobr_wasm_bg.wasm');
  const start = performance.now();
  const wasm = await import(pathToFileURL(resolve(directory, 'pobr_wasm.js')).href);
  const moduleImportMs = performance.now() - start;
  const wasmStarted = performance.now();
  wasm.initSync({ module: bytes });
  const wasmInitializationMs = performance.now() - wasmStarted;
  const stagingStarted = performance.now();
  for (const [file, text] of files) wasm.stageDataFile(file, text);
  const dataStagingMs = performance.now() - stagingStarted;
  const initializationStarted = performance.now();
  wasm.initStagedData();
  const dataInitializationMs = performance.now() - initializationStarted;
  const initializationMs = performance.now() - start;
  if (wasm.schemaVersion() !== receipt.schemaVersion) throw new Error(`Compiled schema differs from receipt: ${directory}`);
  return { wasm, initializationMs, artifactReadMs, artifactVerificationMs,
    phases: { moduleImportMs, wasmInitializationMs, dataStagingMs, dataInitializationMs },
    buildReceiptSha256: sha256(receiptBytes),
    compiled: { commit: receipt.commit, dirty: receipt.dirty, sourceFingerprint: receipt.sourceFingerprint,
      profile: receipt.profile, target: receipt.target, features: receipt.features, buildInputs: receipt.buildInputs },
    wasmSha256: sha256(bytes) };
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
    // Exactly one calculation per fresh engine before any warmup or earlier API calculation.
    // In paired mode both initializations precede this call, so no startup wall time is claimed.
    if (scenario === 'single') {
      for (const engine of engines) {
        if (engine.firstCalculation) continue;
        const start = performance.now();
        const response = engine.wasm.calculateBuildJson(json);
        const calculationMs = performance.now() - start;
        const wallFromScriptMs = engines.length === 1 ? performance.now() - started : null;
        const parsed = JSON.parse(response);
        if (!Array.isArray(parsed.stats) || !parsed.provenance?.data_sha256 || parsed.error) {
          throw new Error(`Invalid first calculation result: ${fixture}`);
        }
        engine.firstCalculation = { fixture, calculationMs, wallFromScriptMs,
          requestSha256: sha256(json), responseSha256: sha256(response), provenance: parsed.provenance };
        if (parsed.breakdown_descriptors === undefined && engine.wasm.breakdownDescriptorsJson) {
          parsed.breakdown_descriptors = JSON.parse(engine.wasm.breakdownDescriptorsJson());
        }
        expected.set(engine.name, normalize(parsed));
      }
    }
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
        const raw = JSON.parse(response);
        // Current engines leave the detail-kind table out of a response and offer it once;
        // older packages inline it. Put it back so both compare like with like.
        if (method === 'calculateBuildJson' && raw.breakdown_descriptors === undefined && engine.wasm.breakdownDescriptorsJson) {
          raw.breakdown_descriptors = JSON.parse(engine.wasm.breakdownDescriptorsJson());
        }
        const parsed = normalize(raw);
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
    const result = { fixture, fixtureSha256: sha256(code), scenario, iterations, equivalent: engines.length === 2 ? true : null,
      engines: engines.map(engine => ({ name: engine.name, medianMs: median(samples.get(engine.name)), samplesMs: samples.get(engine.name) })) };
    measurements.push(result);
    console.log(JSON.stringify({ fixture, scenario, equivalent: result.equivalent,
      medians: Object.fromEntries(result.engines.map(engine => [engine.name, Number(engine.medianMs.toFixed(3))])) }));
  }
}

await mkdir(dirname(outputPath), { recursive: true });
await writeFile(outputPath, JSON.stringify({ format: 'pobr-calc-performance/v2', dataVersion: manifest.version,
  host: { node: process.version, platform: platform(), arch: arch(), release: release(), cpu: cpus()[0]?.model },
  method: {
    startup: 'Single-engine wall time starts inside this Node script, includes file reads, integrity checks, module import, WASM/data initialization and the first calculation. Excludes Node process startup. Paired runs report no startup wall time.',
    dataLoading: 'stageDataFile UTF-8 text staging plus initStagedData (including Rust integrity verification), matching the earlier benchmark. It does not measure browser fetch/byte staging or initStagedDataVerified.',
    cache: 'No API calculation precedes firstCalculation. Warm samples use different JSON whitespace to bypass endpoint response keys; internal parsed-data or calculation memoization is not disabled.',
    memory: 'peakProcessRssBytes is the Node process high-water RSS through the end of the run, including harness buffers and both engines if paired. It is not WASM-only memory.',
    limits: 'Filesystem/OS caches are uncontrolled. These are Node observations, not cold-disk, browser, network, p95 startup, numerical correctness or SLA evidence. Recorded build receipts identify artifacts; their source checkout and build environment are not reverified here.',
  },
  data: { manifestSha256: sha256(manifestBytes), dataReadMs, dataVerificationMs, inventory: dataInventory },
  engines: engines.map(({ wasm, ...engine }) => ({ ...engine, schema: wasm.schemaVersion() })),
  peakProcessRssBytes: process.resourceUsage().maxRSS * 1024,
  measurements }, null, 2) + '\n', { flag: 'wx' });
