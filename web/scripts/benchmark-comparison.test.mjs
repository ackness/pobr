import test from 'node:test';
import assert from 'node:assert/strict';
import {
  parseOptions, summarize, speedRatio, validateOutput, normalizeOutput, measureComparison,
} from './benchmark-comparison.mjs';

const pobrOutput = (life = 1234, unsupported = ['first', 'second']) => ({
  stats: [{ id: 'Life', value: life }, { id: 'TotalDPS', value: 2500 }, { id: 'TotalEHP', value: 9000 }],
  unsupported_modifiers: unsupported,
  item_errors: [{ slot: 'Helmet', message: 'unparsed item' }],
  main_skill: null,
});
const luaOutput = (life = 5678) => ({ Life: life, TotalDPS: 3500, _allocatedNodes: [10, 20] });
const builds = ['synthetic-alpha', 'synthetic-beta'].map((fixture, index) => ({
  fixture, code: `encoded-build-${index}`, xml: `<PathOfBuilding id="${index}"/>`, sha256: String(index).repeat(64),
}));

// These engines exercise orchestration invariants, not benchmark performance or
// engine correctness. They never echo a request and no timing result is asserted.
function policyEngines(onCall = () => {}) {
  return [
    { name: 'pobr', async calculate(request) {
      onCall('pobr', request);
      return JSON.stringify(pobrOutput());
    } },
    { name: 'pob-web', async calculate(request) {
      onCall('pob-web', request);
      return JSON.stringify(luaOutput());
    } },
  ];
}

test('options accept inclusive count boundaries and explicit paths', () => {
  for (const [iterations, warmup] of [[3, 1], [100, 20]]) {
    const result = parseOptions(['--iterations', String(iterations), '--warmup', String(warmup),
      '--reference-dir', 'reference', '--wasm-dir', 'wasm', '--output', 'result.json']);
    assert.equal(result.iterations, iterations);
    assert.equal(result.warmup, warmup);
    assert.equal(result.referenceDir, 'reference');
    assert.equal(result.wasmDir, 'wasm');
    assert.equal(result.output, 'result.json');
  }
  assert.equal(parseOptions(['--help']).help, true);
});

test('options reject unknown, duplicate, missing and noninteger values even with help', () => {
  const invalid = [
    ['--unknown'], ['--prepare-reference'], ['--help', '--unknown'], ['--help', '--help'],
    ['--output', 'one', '--output', 'two'], ['--iterations', '3', '--iterations', '4'],
    ['--reference-dir'], ['--wasm-dir', '--help'], ['--output', ''], ['--output', '   '],
    ['--iterations', '2'], ['--iterations', '101'], ['--warmup', '0'], ['--warmup', '21'],
    ['--iterations', '-3'], ['--iterations', '3.0'], ['--iterations', '3.5'],
    ['--iterations', '3e1'], ['--warmup', 'NaN'], ['--iterations', 'Infinity'],
    ['--iterations', '9007199254740993'], ['--iterations=3'], ['positional'],
  ];
  for (const args of invalid) assert.throws(() => parseOptions(args), undefined, args.join(' '));
});

test('statistics calculate odd/even medians and nearest-rank p95 without mutating samples', () => {
  const odd = [9, 1, 5];
  assert.deepEqual(summarize(odd), { medianMs: 5, p95Ms: 9 });
  assert.deepEqual(odd, [9, 1, 5]);
  assert.deepEqual(summarize([8, 2, 6, 4]), { medianMs: 5, p95Ms: 8 });
  assert.deepEqual(summarize(Array.from({ length: 20 }, (_, index) => index + 1)), {
    medianMs: 10.5, p95Ms: 19,
  });
  assert.deepEqual(summarize([0]), { medianMs: 0, p95Ms: 0 });
  for (const invalid of [[], [NaN], [Infinity], [-1], ['3'], null]) {
    assert.throws(() => summarize(invalid));
  }
});

test('speed ratio is Lua time divided by Rust time, so larger than one means Rust faster', () => {
  assert.equal(speedRatio(30, 10), 3);
  assert.equal(speedRatio(5, 10), 0.5);
  for (const pair of [[0, 1], [1, 0], [-1, 2], [Infinity, 2], [2, NaN]]) {
    assert.throws(() => speedRatio(...pair));
  }
});

test('validation maps real backend shapes and retains unsupported diagnostics', () => {
  const raw = pobrOutput();
  const validated = validateOutput(JSON.stringify(raw), 'pobr');
  assert.deepEqual(validated.output, raw);
  assert.deepEqual(validated.selectedStats, { Life: 1234, TotalDPS: 2500, CombinedDPS: null, TotalEHP: 9000 });
  assert.deepEqual(validated.diagnostics, {
    unsupported_modifiers: ['first', 'second'], item_errors: raw.item_errors,
  });
  assert.deepEqual(validateOutput(JSON.stringify(luaOutput()), 'pob-web').selectedStats, {
    Life: 5678, TotalDPS: 3500, CombinedDPS: null, TotalEHP: null,
  });
});

test('validation rejects invalid shapes, backend failures, missing Life and nonpositive Life', () => {
  for (const raw of [null, {}, '{', 'null', '[]', '42']) {
    assert.throws(() => validateOutput(raw, 'pobr'));
  }
  for (const life of [null, 0, -1, '100', Infinity, NaN]) {
    assert.throws(() => validateOutput(JSON.stringify(pobrOutput(life)), 'pobr'));
    assert.throws(() => validateOutput(JSON.stringify(luaOutput(life)), 'pob-web'));
  }
  assert.throws(() => validateOutput('{"Life":1e400}', 'pob-web'));
  assert.throws(() => validateOutput('{"stats":[]}', 'pobr'));
  assert.throws(() => validateOutput('{"stats":{"Life":123}}', 'pobr'));
  assert.throws(() => validateOutput('{"Life":123}', 'pobr'));
  for (const failure of [
    { error: 'Lua load failed' }, { errors: ['failed'] }, { exception: 'bad bridge' },
    { bridge_error: 'failed' }, { success: false }, { ok: false },
  ]) {
    assert.throws(() => validateOutput(JSON.stringify({ ...luaOutput(), ...failure }), 'pob-web'));
  }
  const duplicate = pobrOutput();
  duplicate.stats.push({ id: 'Life', value: 1234 });
  assert.throws(() => validateOutput(JSON.stringify(duplicate), 'pobr'));
  assert.throws(() => validateOutput(JSON.stringify({ ...luaOutput(), TotalDPS: 'unknown' }), 'pob-web'));
  assert.throws(() => validateOutput(JSON.stringify({ ...pobrOutput(), item_errors: 'invalid' }), 'pobr'));
});

test('normalization ignores object keys and unsupported order, not other arrays or values', () => {
  assert.deepEqual(normalizeOutput(pobrOutput(1234, ['first', 'second'])),
    normalizeOutput(pobrOutput(1234, ['second', 'first'])));
  assert.deepEqual(normalizeOutput({ b: 1, a: 2 }), normalizeOutput({ a: 2, b: 1 }));
  assert.notDeepEqual(normalizeOutput({ item_errors: ['a', 'b'] }), normalizeOutput({ item_errors: ['b', 'a'] }));
  assert.notDeepEqual(normalizeOutput(luaOutput()), normalizeOutput({ ...luaOutput(), _allocatedNodes: [20, 10] }));
  assert.notDeepEqual(normalizeOutput(pobrOutput()), normalizeOutput(pobrOutput(999)));
});

test('all warmups and measured calls have unique nonces across builds and alternating order', async () => {
  const calls = [];
  const notified = [];
  const result = await measureComparison({ engines: policyEngines((name, request) => calls.push({ name, ...request })),
    builds, iterations: 3, warmup: 1, onMeasurement: entry => notified.push(entry) });
  assert.equal(calls.length, 16);
  assert.equal(new Set(calls.map(call => call.nonce)).size, 16);
  assert.deepEqual(calls.map(call => call.name), [
    'pobr', 'pob-web', 'pob-web', 'pobr', 'pobr', 'pob-web', 'pob-web', 'pobr',
    'pobr', 'pob-web', 'pob-web', 'pobr', 'pobr', 'pob-web', 'pob-web', 'pobr',
  ]);
  assert.equal(calls[0].code, builds[0].code);
  assert.equal(calls[8].xml, builds[1].xml);
  assert.equal(result.length, 2);
  assert.deepEqual(notified, result);
  for (const entry of result) {
    assert.equal(entry.speedup.numerator, 'pob-web');
    assert.equal(entry.speedup.denominator, 'pobr');
    for (const engine of entry.engines) assert.equal(engine.samplesMs.length, 3);
    assert.notEqual(entry.engines[0].selectedStats.Life, entry.engines[1].selectedStats.Life);
    assert.deepEqual(entry.engines[0].diagnostics.item_errors, pobrOutput().item_errors);
  }
});

test('repeat stability includes warmup and permits only unsupported ordering changes', async () => {
  let count = 0;
  const engines = policyEngines();
  engines[0].calculate = async () => JSON.stringify(pobrOutput(1234, ++count % 2 ? ['first', 'second'] : ['second', 'first']));
  await measureComparison({ engines, builds: builds.slice(0, 1), iterations: 3, warmup: 1 });
  count = 0;
  engines[0].calculate = async () => JSON.stringify(pobrOutput(++count === 1 ? 1234 : 1235));
  await assert.rejects(measureComparison({ engines, builds, iterations: 3, warmup: 1 }), /output changed/);
});

test('calculation and validation failures reject without a fast success report', async () => {
  for (const calculate of [async () => { throw new Error('bridge rejected'); }, async () => '{"error":"failed"}',
    async () => JSON.stringify(pobrOutput(0))]) {
    const engines = policyEngines();
    engines[0].calculate = calculate;
    let reported = false;
    await assert.rejects(measureComparison({ engines, builds, iterations: 3, warmup: 1,
      onMeasurement: () => { reported = true; } }));
    assert.equal(reported, false);
  }
});

test('measurement rejects malformed requests and out-of-range counts before engine calls', async () => {
  let calls = 0;
  const engines = policyEngines(() => { calls += 1; });
  for (const request of [
    { iterations: 2 }, { warmup: 0 }, { iterations: 3.5 }, { builds: [] },
    { builds: [{ ...builds[0], sha256: 'bad' }] }, { builds: [builds[0], builds[0]] },
    { builds: [{ ...builds[0], xml: '' }] }, { engines: [engines[0]] },
    { engines: [engines[0], engines[0]] },
  ]) {
    await assert.rejects(measureComparison({ engines, builds, ...request }));
  }
  assert.equal(calls, 0);
});
