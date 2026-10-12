// Opt-in real-engine benchmark; no UI, network or filesystem I/O in calculation samples.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { cpus, platform, arch, release } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { inflateSync } from 'node:zlib';
import { parseOptions, measureComparison, measureEngine } from './benchmark-comparison.mjs';
import { discoverBuilds } from './benchmark-builds.mjs';
import { loadPobWeb } from './benchmark-pob-web.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

async function loadPobr(directory) {
  const manifest = JSON.parse(await readFile(resolve(root, 'web/public/data/manifest.json'), 'utf8'));
  const files = await Promise.all(manifest.files.map(async file => {
    const versionRoot = resolve(root, 'web/public/data', manifest.version);
    const path = resolve(versionRoot, file);
    if (!path.startsWith(versionRoot + '/')) throw new Error(`Unsafe data path: ${file}`);
    return [file, await readFile(path, 'utf8')];
  }));
  const bytes = await readFile(resolve(directory, 'pobr_wasm_bg.wasm'));
  const js = await readFile(resolve(directory, 'pobr_wasm.js'));
  const receipt = JSON.parse(await readFile(resolve(directory, 'build-receipt.json'), 'utf8'));
  for (const [file, content] of [['pobr_wasm_bg.wasm', bytes], ['pobr_wasm.js', js]]) {
    if (receipt.files[file] !== sha256(content)) throw new Error(`WASM receipt hash mismatch: ${file}`);
  }
  const start = performance.now();
  const wasm = await import(pathToFileURL(resolve(directory, 'pobr_wasm.js')).href);
  wasm.initSync({ module: bytes });
  for (const [file, text] of files) wasm.stageDataFile(file, text);
  wasm.initStagedData();
  const initializationMs = performance.now() - start;
  if (wasm.schemaVersion() !== receipt.schemaVersion) throw new Error('WASM receipt schema mismatch');
  let requestIndex = 0;
  return {
    name: 'pobr', initializationMs,
    provenance: {
      commit: receipt.commit, dirty: receipt.dirty, sourceFingerprint: receipt.sourceFingerprint,
      schemaVersion: wasm.schemaVersion(), dataVersion: manifest.version,
      rustc: receipt.rustc, wasmPack: receipt.wasmPack,
      files: [
        { name: 'pobr_wasm_bg.wasm', bytes: bytes.length, sha256: sha256(bytes) },
        { name: 'pobr_wasm.js', bytes: js.length, sha256: sha256(js) },
        ...files.map(([name, text]) => ({ name, bytes: Buffer.byteLength(text), sha256: sha256(text) })),
      ],
    },
    calculate({ code }) {
      // This fresh WASM instance has no earlier requests; never reuse an exact JSON cache key.
      return wasm.calculateBuildJson(JSON.stringify({ pob_code: code }) + ' '.repeat(++requestIndex));
    },
    close() {},
  };
}

async function main() {
  const options = parseOptions(process.argv.slice(2));
  if (options.help) {
    console.log(`Usage: node web/scripts/bench-pob-web.mjs [options]
  --reference-dir <dir>  Verified pob-web snapshot (default: .cache/pob-web-benchmark)
  --wasm-dir <dir>       PoBR bindings with build receipt (default: web/src/wasm/pkg)
  --iterations <3..100>  Measured calls per build/engine (default: 9)
  --warmup <1..20>       Untimed calls per build/engine (default: 2)
  --output <file>        JSON report (default: .cache/pob-web-performance.json)
  --pobr-only            Measure PoBR alone on every build: no reference snapshot, no speed ratio.
                         For builds the Lua reference rejects. Default report: .cache/pobr-performance.json

Prepare the reference first with node web/scripts/benchmark-pob-web.mjs --help.
Selected historical fixtures and all latest-season performance fixtures are measured.
This measures import + calculation, not cached reads, incremental edits or numerical parity.`);
    return;
  }
  const builds = await discoverBuilds();
  const engines = [];
  try {
    engines.push(await loadPobr(resolve(root, options.wasmDir)));
    if (!options.pobrOnly) {
      const reference = await loadPobWeb({ directory: resolve(root, options.referenceDir) });
      engines.push({
        ...reference,
        calculate(input) {
          // Match PoBR's code-to-stats boundary; pob-web's worker accepts decoded XML.
          const xml = inflateSync(Buffer.from(input.code, 'base64url')).toString('utf8');
          return reference.calculate({ ...input, xml });
        },
      });
    }
    const measurements = [];
    for (const build of builds) {
      try {
        const request = { builds: [build], iterations: options.iterations, warmup: options.warmup };
        const [measurement] = options.pobrOnly
          ? await measureEngine({ engine: engines[0], ...request })
          : await measureComparison({ engines, ...request });
        Object.assign(measurement, { character: build.character, source: build.source, corpus: build.corpus });
        measurements.push(measurement);
        console.log(JSON.stringify({
          fixture: build.fixture, character: build.character, source: build.source, corpus: build.corpus,
          speedup: measurement.speedup,
          engines: measurement.engines.map(({ name, medianMs, p95Ms, selectedStats, diagnostics }) => ({
            name, medianMs, p95Ms, selectedStats,
            diagnosticCounts: Object.fromEntries(Object.entries(diagnostics).map(([key, values]) => [key, values.length])),
          })),
        }));
      } catch (error) {
        // Keep failed fixtures visible, but never publish a speed ratio for partial calculations.
        measurements.push({ fixture: build.fixture, sha256: build.sha256, character: build.character,
          source: build.source, corpus: build.corpus, error: error.message });
        console.error(`${build.fixture}: ${error.message}`);
      }
    }
    const report = {
      measuredAt: new Date().toISOString(),
      host: { node: process.version, platform: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model },
      method: options.pobrOnly ? {
        scenario: 'import-and-calculate', mode: 'pobr-only', iterations: options.iterations, warmup: options.warmup,
        timing: 'PoBR calculateBuildJson only: request serialization, unique legal JSON whitespace, code inflation, import, calculation and response serialization. Harness JSON validation and repeat-stability checks outside samples. No reference calls are interleaved.',
        initialization: 'One observed PoBR initialization, including ES module import I/O, from preloaded WASM/data bytes. Not a startup distribution or network measurement.',
        parityAsserted: false,
        limitations: 'Single-engine observation: no cross-engine speedup, numerical parity or correct main-skill selection is asserted, and it is not comparable call-for-call with a paired run on another host or day. No browser rendering, download time, incremental editing or runtime memory measurement.',
      } : {
        scenario: 'import-and-calculate', mode: 'paired', iterations: options.iterations, warmup: options.warmup,
        timing: 'PoB code to JSON result. Includes code inflation for both engines (Rust for PoBR; Node zlib before pob-web XML import), adapter validation and Node Worker transport for pob-web. Harness JSON validation outside samples. Alternating engine order; unique PoBR request strings bypass the response cache.',
        initialization: 'One observed initialization per engine, including ES module import I/O, from preloaded WASM/data bytes. Excludes reference worker creation and artifact reads/hash checks. Not a startup distribution or network measurement.',
        parityAsserted: false,
        limitations: 'Different engine implementations, output contracts and game-data/feature coverage. pob-web also constructs headless UI state during import. No browser rendering or browser Worker transport, download time, incremental editing or runtime memory comparison.',
      },
      builds: builds.map(({ fixture, sha256, xmlSha256, character, source, corpus }) =>
        ({ fixture, sha256, xmlSha256, character, source, corpus })),
      engines: engines.map(({ name, initializationMs, provenance }) => ({ name, initializationMs, provenance })),
      measurements,
    };
    const output = resolve(root, options.output);
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, JSON.stringify(report, null, 2) + '\n');
    console.log(`Report: ${output}`);
    if (measurements.some(measurement => measurement.error)) process.exitCode = 1;
  } finally {
    for (const engine of engines.reverse()) await engine.close();
  }
}

main().catch(error => {
  console.error(error.stack ?? String(error));
  process.exitCode = 1;
});
