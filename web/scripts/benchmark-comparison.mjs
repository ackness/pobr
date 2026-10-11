import { randomUUID } from 'node:crypto';
import { performance } from 'node:perf_hooks';

export function parseOptions(args) {
  const options = {
    referenceDir: '.cache/pob-web-benchmark',
    wasmDir: 'web/src/wasm/pkg',
    output: '.cache/pob-web-performance.json',
    iterations: 9,
    warmup: 2,
    help: false,
  };
  const flags = new Map([
    ['--reference-dir', 'referenceDir'], ['--wasm-dir', 'wasmDir'],
    ['--output', 'output'], ['--iterations', 'iterations'], ['--warmup', 'warmup'],
    ['--help', 'help'],
  ]);
  const seen = new Set();
  for (let index = 0; index < args.length; index += 1) {
    const flag = args[index];
    const key = flags.get(flag);
    if (!key) throw new Error(`Unknown option: ${flag}`);
    if (seen.has(flag)) throw new Error(`Duplicate option: ${flag}`);
    seen.add(flag);
    if (key === 'help') {
      options.help = true;
      continue;
    }
    const value = args[++index];
    if (typeof value !== 'string' || value.trim() === '' || value.startsWith('--')) {
      throw new Error(`Missing value for ${flag}`);
    }
    if (key === 'iterations' || key === 'warmup') {
      if (!/^\d+$/.test(value)) throw new Error(`${flag} requires an integer`);
      options[key] = Number(value);
    } else {
      options[key] = value;
    }
  }
  validateCounts(options.iterations, options.warmup);
  return options;
}

function validateCounts(iterations, warmup) {
  if (!Number.isInteger(iterations) || iterations < 3 || iterations > 100) {
    throw new Error('iterations must be an integer between 3 and 100');
  }
  if (!Number.isInteger(warmup) || warmup < 1 || warmup > 20) {
    throw new Error('warmup must be an integer between 1 and 20');
  }
}

// Median interpolates the central pair; p95 uses the nearest-rank definition.
export function summarize(samples) {
  if (!Array.isArray(samples) || samples.length === 0
      || samples.some(value => !Number.isFinite(value) || value < 0)) {
    throw new Error('Samples must be a nonempty array of finite nonnegative milliseconds');
  }
  const sorted = [...samples].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return {
    medianMs: sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2,
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
  };
}

// Object key order is not semantic. Only unsupported modifier ordering is ignored;
// every other array remains order-sensitive, including item errors and stat rows.
export function normalizeOutput(value, key = '') {
  if (Array.isArray(value)) {
    const normalized = value.map(entry => normalizeOutput(entry));
    return key === 'unsupported_modifiers'
      ? normalized.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)))
      : normalized;
  }
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map(name => [name, normalizeOutput(value[name], name)]));
  }
  return value;
}

export function validateOutput(raw, name) {
  if (typeof raw !== 'string') throw new Error(`${name}: expected a raw JSON response string`);
  let output;
  try {
    output = JSON.parse(raw);
  } catch (error) {
    throw new Error(`${name}: invalid JSON response`, { cause: error });
  }
  if (output === null || typeof output !== 'object' || Array.isArray(output)) {
    throw new Error(`${name}: expected a JSON object response`);
  }
  for (const key of ['error', 'errors', 'exception', 'bridge_error']) {
    if (Object.hasOwn(output, key) && output[key] !== null && output[key] !== false
        && !(Array.isArray(output[key]) && output[key].length === 0)) {
      throw new Error(`${name}: backend ${key}: ${JSON.stringify(output[key])}`);
    }
  }
  if (output.success === false || output.ok === false) throw new Error(`${name}: backend reported failure`);
  let stats;
  if (name === 'pob-web') {
    stats = output;
  } else {
    if (!Array.isArray(output.stats)) throw new Error(`${name}: missing PoBR stats array`);
    stats = {};
    for (const row of output.stats) {
      if (!row || typeof row.id !== 'string' || Object.hasOwn(stats, row.id)) {
        throw new Error(`${name}: invalid or duplicate stat row`);
      }
      stats[row.id] = row.value;
    }
  }
  if (typeof stats.Life !== 'number' || !Number.isFinite(stats.Life) || stats.Life <= 0) {
    throw new Error(`${name}: Life must be a finite positive number`);
  }
  const selectedStats = {};
  for (const id of ['Life', 'TotalDPS', 'CombinedDPS', 'TotalEHP']) {
    const value = stats[id];
    if (value === undefined) {
      selectedStats[id] = null;
    } else {
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        throw new Error(`${name}: ${id} must be finite when present`);
      }
      selectedStats[id] = value;
    }
  }
  const diagnostics = {};
  for (const key of ['unsupported_modifiers', 'item_errors']) {
    if (Object.hasOwn(output, key)) {
      if (!Array.isArray(output[key])) throw new Error(`${name}: ${key} must be an array`);
      diagnostics[key] = output[key];
    }
  }
  return { output, selectedStats, diagnostics, normalized: JSON.stringify(normalizeOutput(output)) };
}

export function speedRatio(numeratorMs, denominatorMs) {
  if (!Number.isFinite(numeratorMs) || !Number.isFinite(denominatorMs)
      || numeratorMs <= 0 || denominatorMs <= 0) {
    throw new Error('Speed ratio requires finite positive median times');
  }
  return numeratorMs / denominatorMs;
}

export async function measureComparison({
  engines, builds, iterations = 9, warmup = 2, now = () => performance.now(), onMeasurement,
}) {
  validateCounts(iterations, warmup);
  if (!Array.isArray(engines) || engines.length !== 2
      || engines.some(engine => !engine || typeof engine.name !== 'string' || !engine.name
        || typeof engine.calculate !== 'function') || engines[0].name === engines[1].name) {
    throw new Error('Comparison requires two uniquely named calculation engines');
  }
  if (!Array.isArray(builds) || builds.length === 0) throw new Error('Comparison requires builds');
  const seen = new Set();
  for (const build of builds) {
    if (!build || typeof build.fixture !== 'string' || !build.fixture || seen.has(build.fixture)
        || typeof build.code !== 'string' || !build.code.trim()
        || typeof build.xml !== 'string' || !build.xml.trim()
        || typeof build.sha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(build.sha256)) {
      throw new Error('Each build requires a unique fixture, code, XML, and SHA-256');
    }
    seen.add(build.fixture);
  }
  const runId = randomUUID();
  let call = 0;
  let round = 0;
  const measurements = [];
  for (const build of builds) {
    const records = engines.map(engine => ({ name: engine.name, samplesMs: [] }));
    const baselines = new Map();
    for (let iteration = -warmup; iteration < iterations; iteration += 1) {
      const order = round++ % 2 === 0 ? [0, 1] : [1, 0];
      for (const engineIndex of order) {
        const engine = engines[engineIndex];
        const start = now();
        let raw;
        try {
          raw = await engine.calculate({ code: build.code, xml: build.xml, nonce: `${runId}:${call++}` });
        } catch (error) {
          throw new Error(`${build.fixture}/${engine.name}: ${error.message}`, { cause: error });
        }
        const elapsed = now() - start;
        const validated = validateOutput(raw, engine.name);
        if (baselines.has(engine.name) && baselines.get(engine.name) !== validated.normalized) {
          throw new Error(`${build.fixture}/${engine.name}: output changed between repeated calculations`);
        }
        baselines.set(engine.name, validated.normalized);
        if (!Number.isFinite(elapsed) || elapsed < 0) throw new Error('Clock produced an invalid elapsed time');
        const record = records[engineIndex];
        if (!Object.hasOwn(record, 'output')) {
          record.output = validated.output;
          record.selectedStats = validated.selectedStats;
          record.diagnostics = validated.diagnostics;
        }
        if (iteration >= 0) record.samplesMs.push(elapsed);
      }
    }
    for (const record of records) Object.assign(record, summarize(record.samplesMs));
    const measurement = {
      fixture: build.fixture,
      sha256: build.sha256,
      engines: records,
      speedup: {
        numerator: records[1].name,
        denominator: records[0].name,
        ratio: speedRatio(records[1].medianMs, records[0].medianMs),
      },
    };
    measurements.push(measurement);
    if (onMeasurement) await onMeasurement(measurement);
  }
  return measurements;
}
